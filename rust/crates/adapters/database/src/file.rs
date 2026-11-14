//! The store on a disk of its own, for the runtime that has no database to talk to.
//!
//! The statements the store writes are fixed in shape, so the graph that answers them fits in memory. This keeps that
//! graph and writes it out whenever a statement changes what is held, which is what lets the on-device runtime keep a
//! Book across restarts without Neo4j. Every write is a whole-file replace of the snapshot, taken under a lock so two
//! callers cannot interleave, and landing through a rename so a crash mid-write leaves the previous file intact.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use crate::fake::MemoryGraph;
use crate::{Graph, StoreError};

pub struct FileGraph {
    held: MemoryGraph,
    path: PathBuf,
    /// One writer at a time: the snapshot is the whole file, so two half-writes would lose records.
    writing: Mutex<()>,
}

impl FileGraph {
    /// Open the graph held in a file, or an empty one about to be held there.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let held = MemoryGraph::new();
        if path.exists() {
            let text = std::fs::read_to_string(&path).map_err(|error| {
                StoreError::Graph(format!("cannot read {}: {error}", path.display()))
            })?;
            if !text.trim().is_empty() {
                let snapshot: Value = serde_json::from_str(&text).map_err(|error| {
                    StoreError::Graph(format!("{} is not a store: {error}", path.display()))
                })?;
                held.restore(&snapshot);
            }
        }
        Ok(Self {
            held,
            path,
            writing: Mutex::new(()),
        })
    }

    /// Everything held, for a caller that wants to look without going through a statement.
    pub fn snapshot(&self) -> Value {
        self.held.snapshot()
    }

    fn write_out(&self) -> Result<(), StoreError> {
        let _only_one = self
            .writing
            .lock()
            .expect("the write lock is never poisoned");
        let text = self.held.snapshot().to_string();
        let temporary = self.path.with_extension("writing");
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                StoreError::Graph(format!("cannot make {}: {error}", parent.display()))
            })?;
        }
        let mut file = std::fs::File::create(&temporary).map_err(|error| {
            StoreError::Graph(format!("cannot write {}: {error}", temporary.display()))
        })?;
        file.write_all(text.as_bytes()).map_err(|error| {
            StoreError::Graph(format!("cannot write {}: {error}", temporary.display()))
        })?;
        file.sync_all().map_err(|error| {
            StoreError::Graph(format!("cannot flush {}: {error}", temporary.display()))
        })?;
        std::fs::rename(&temporary, &self.path).map_err(|error| {
            StoreError::Graph(format!("cannot keep {}: {error}", self.path.display()))
        })?;
        Ok(())
    }
}

/// A statement that leaves the graph different from how it found it. A read is not written back.
fn changes_something(cypher: &str) -> bool {
    ["CREATE", "DELETE", "SET ", "MERGE"]
        .iter()
        .any(|word| cypher.contains(word))
}

impl Graph for FileGraph {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        let rows = self.held.run(cypher, params)?;
        if changes_something(cypher) {
            self.write_out()?;
        }
        Ok(rows)
    }

    fn all_edges(&self) -> Result<Vec<(String, String, String)>, StoreError> {
        self.held.all_edges()
    }

    fn create_edge(&self, from: &str, label: &str, to: &str) -> Result<(), StoreError> {
        self.held.create_edge(from, label, to)?;
        self.write_out()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BookScope, RecordKind, Records};
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vimbai-file-graph-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir.join("book.json")
    }

    const INVOICE: RecordKind = RecordKind::new("Invoice", "OWNS_INVOICE");

    #[test]
    fn a_record_written_to_a_file_is_there_after_the_store_is_reopened() {
        let path = scratch("record");
        let scope = BookScope::new("user-1", "book-1");
        let id = {
            let graph = FileGraph::open(&path).expect("open");
            let made = Records::new(&graph, INVOICE)
                .create(
                    &scope,
                    &json!({"number": "INV-1", "total_amount": "120.00"}),
                )
                .expect("create");
            made["id"].as_str().expect("id").to_string()
        };
        assert!(path.exists(), "nothing was written");

        let reopened = FileGraph::open(&path).expect("reopen");
        let held = Records::new(&reopened, INVOICE)
            .get(&scope, &id)
            .expect("read back");
        assert_eq!(held["number"], json!("INV-1"));
        assert_eq!(held["total_amount"], json!("120.00"));
    }

    #[test]
    fn a_deleted_record_stays_deleted_and_another_book_still_cannot_see_it() {
        let path = scratch("delete");
        let mine = BookScope::new("user-1", "book-1");
        let theirs = BookScope::new("user-2", "book-2");
        let graph = FileGraph::open(&path).expect("open");
        let made = Records::new(&graph, INVOICE)
            .create(&mine, &json!({"number": "INV-2"}))
            .expect("create");
        let id = made["id"].as_str().expect("id").to_string();
        assert!(
            Records::new(&graph, INVOICE).get(&theirs, &id).is_err(),
            "another book read it"
        );
        Records::new(&graph, INVOICE)
            .delete(&mine, &id)
            .expect("delete");
        drop(graph);

        let reopened = FileGraph::open(&path).expect("reopen");
        assert!(Records::new(&reopened, INVOICE).get(&mine, &id).is_err());
    }

    #[test]
    fn the_file_is_a_store_a_fresh_open_can_read_without_a_database() {
        let path = scratch("snapshot");
        let graph = FileGraph::open(&path).expect("open");
        let empty = graph.snapshot();
        assert_eq!(empty["nodes"].as_array().map(Vec::len), Some(0));
        Records::new(&graph, INVOICE)
            .create(
                &BookScope::new("user-1", "book-1"),
                &json!({"number": "INV-3"}),
            )
            .expect("create");
        drop(graph);
        let reopened = FileGraph::open(&path).expect("reopen");
        let held = reopened.snapshot();
        assert_eq!(held["nodes"].as_array().map(Vec::len), Some(1));
        assert!(held["edges"].is_array(), "no edge list in the file");
    }
}
