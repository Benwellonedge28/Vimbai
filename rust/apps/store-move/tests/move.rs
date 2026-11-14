//! The move, as an operator runs it: source, destination, and a verdict.
//!
//! Two file stores make this a real move - different directories, written by the same code path a server store uses - so what
//! is tested is the tool, not a stub beside it.

#![forbid(unsafe_code)]

use serde_json::{json, Value};

use vimbai_store::file::FileGraph;
use vimbai_store::{BookScope, Records, EXPENSE, JOURNAL_ENTRY};
use vimbai_store_move::move_all;

fn a_book_at(path: &std::path::Path) -> BookScope {
    let graph = FileGraph::open(path).expect("a store");
    let scope = BookScope::new("sam", "book-swap");
    Records::new(&graph, JOURNAL_ENTRY)
        .create(
            &scope,
            &json!({"id": "entry-9", "reference": "INV-9", "posted": true, "created_at": "2026-11-14T11:00:00Z"}),
        )
        .expect("an entry");
    Records::new(&graph, EXPENSE)
        .create(
            &scope,
            &json!({
                "id": "expense-9",
                "amount": "98765432109876543210.000000001",
                "created_at": "2026-11-14T11:05:00Z"
            }),
        )
        .expect("an expense");
    scope
}

#[test]
fn the_move_is_reported_as_faithful_and_the_records_are_there_afterwards() {
    let dir = std::env::temp_dir().join(format!("vimbai-move-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let source_path = dir.join("source.json");
    let destination_path = dir.join("destination.json");
    let dump_path = dir.join("taken.jsonl");
    std::fs::create_dir_all(&dir).expect("a directory");

    let scope = a_book_at(&source_path);
    let (restored, differences) = move_all(
        &format!("file:{}", source_path.display()),
        &format!("file:{}", destination_path.display()),
        10_000,
        Some(&dump_path),
    )
    .expect("a move");

    assert!(
        differences.is_empty(),
        "the move disagreed with itself: {differences:?}"
    );
    assert_eq!(restored.records, 2);
    assert!(
        dump_path.exists(),
        "the dump was not written, so the move cannot be replayed"
    );

    // read the destination with a fresh handle: the file, not a cache in the tool
    let destination = FileGraph::open(&destination_path).expect("the destination");
    let held = Records::new(&destination, EXPENSE)
        .list(&scope, 10)
        .expect("the expense");
    assert_eq!(
        held[0].get("amount").and_then(Value::as_str),
        Some("98765432109876543210.000000001"),
        "the amount changed in transit"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_store_that_is_neither_a_file_nor_neo4j_is_refused_by_name() {
    let error = move_all("mysql:whatever", "file:/tmp/nowhere.json", 10, None)
        .expect_err("an unknown store was accepted");
    let said = error.to_string();
    assert!(
        said.contains("file:<path>") && said.contains("neo4j"),
        "{said}"
    );
}

#[test]
fn a_memory_store_is_refused_rather_than_silently_emptied() {
    let error = move_all("memory", "file:/tmp/nowhere.json", 10, None)
        .expect_err("a memory store was accepted as a source");
    assert!(
        error
            .to_string()
            .contains("holds nothing once the process ends"),
        "{error}"
    );
}
