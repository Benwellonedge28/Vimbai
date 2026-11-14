//! Swapping the database, proven rather than asserted.
//!
//! The store is a trait, so a deployment can be given a different one. That promise is only worth anything if the data
//! can move, so these tests take a Book that has actually been written the way the engine writes one - entries with journal
//! lines, lines linked to accounts - and move it into a second, different store. What is checked is what a reader sees on
//! the other side: every record identical, every edge present, and the postings walk still answering the same thing.

#![forbid(unsafe_code)]

use serde_json::{json, Value};

use vimbai_store::dump::{Dump, KINDS};
use vimbai_store::fake::MemoryGraph;
use vimbai_store::file::FileGraph;
use vimbai_store::{BookScope, Graph, Records, EXPENSE, JOURNAL_ENTRY, JOURNAL_LINE};

/// One Book, written the way the engine writes one: two postings against two accounts, each with its own line record.
fn a_written_book(graph: &impl Graph) -> BookScope {
    let scope = BookScope::new("person-1", "book-1");

    // an entry and its journal lines in one statement, the way the engine posts: the lines are records of their own,
    // joined to the entry, and each is linked to its account by the side it moves
    let entry = Records::new(graph, JOURNAL_ENTRY)
        .create_with_children(
            &scope,
            &json!({
                "id": "entry-1",
                "reference": "INV-1",
                "description": "training materials",
                "posted": true,
                "created_at": "2026-11-14T09:00:00Z"
            }),
            JOURNAL_LINE,
            &[
                json!({"account_number": "5000", "debit": "1200.0000001", "credit": "0", "side": "debit"}),
                json!({"account_number": "1100", "debit": "0", "credit": "1200.0000001", "side": "credit"}),
            ],
            "entry_id",
        )
        .expect("an entry");
    assert_eq!(entry.get("id").and_then(Value::as_str), Some("entry-1"));

    // a second posting with a deliberately large amount: a swap that loses precision is a swap that lost money
    Records::new(graph, EXPENSE)
        .create(
            &scope,
            &json!({
                "id": "expense-1",
                "amount": "123456789012345678901234567890.123456789",
                "account_number": "5000",
                "created_at": "2026-11-14T10:00:00Z"
            }),
        )
        .expect("an expense");
    scope
}

#[test]
fn a_book_written_in_one_store_arrives_identical_in_another() {
    let source = MemoryGraph::new();
    a_written_book(&source);

    let taken = Dump::take(&source, 10_000).expect("a dump");
    assert!(
        !taken.records.is_empty(),
        "the dump is empty, so it proves nothing"
    );
    assert!(
        !taken.edges.is_empty(),
        "no edges were dumped, so a swap would lose every relationship"
    );

    // through text, because that is the form a swap actually travels in
    let text = taken.to_text();
    let read = Dump::from_text(&text).expect("a dump read back from text");
    assert_eq!(read, taken, "the dump did not survive being written down");

    let destination = MemoryGraph::new();
    let restored = read.restore(&destination).expect("a restore");
    assert_eq!(restored.records, taken.records.len());
    assert_eq!(restored.edges, taken.edges.len());

    let after = Dump::take(&destination, 10_000).expect("a dump of the new store");
    assert_eq!(after.differences(&taken), Vec::<String>::new());
}

#[test]
fn the_dump_carries_the_amounts_to_the_last_digit() {
    let source = MemoryGraph::new();
    a_written_book(&source);
    let text = Dump::take(&source, 10_000).expect("a dump").to_text();
    let destination = MemoryGraph::new();
    Dump::from_text(&text)
        .expect("a dump")
        .restore(&destination)
        .expect("a restore");

    let scope = BookScope::new("person-1", "book-1");
    let held = Records::new(&destination, EXPENSE)
        .list(&scope, 100)
        .expect("an expense");
    assert_eq!(
        held[0].get("amount").and_then(Value::as_str),
        Some("123456789012345678901234567890.123456789"),
        "a thirty-nine digit amount changed on the way across"
    );
}

#[test]
fn a_book_scoped_read_still_works_on_the_other_side() {
    // the point of the owner edge: a restored Book must belong to its person, or the engine cannot read it at all
    let source = MemoryGraph::new();
    let scope = a_written_book(&source);
    let text = Dump::take(&source, 10_000).expect("a dump").to_text();
    let destination = MemoryGraph::new();
    Dump::from_text(&text)
        .expect("a dump")
        .restore(&destination)
        .expect("a restore");

    let entries = Records::new(&destination, JOURNAL_ENTRY)
        .list(&scope, 100)
        .expect("a listing");
    assert_eq!(
        entries.len(),
        1,
        "the restored entry is not reachable from its Book"
    );

    let lines = Records::new(&destination, JOURNAL_LINE)
        .list(&scope, 100)
        .expect("the lines");
    assert_eq!(lines.len(), 2, "the journal lines did not arrive");

    // and the edges that make a line part of its entry, not a row of numbers that happens to agree
    let links: Vec<(String, String, String)> = destination.all_edges().expect("edges");
    assert!(
        links
            .iter()
            .any(|(from, label, _)| from == "entry-1" && label == "HAS_LINE"),
        "the entry lost its lines: {links:?}"
    );
}

#[test]
fn a_swap_between_two_different_stores_keeps_the_book_whole() {
    // memory to disk: two implementations of the trait, and the one that travels is a file
    let source = MemoryGraph::new();
    let scope = a_written_book(&source);
    let text = Dump::take(&source, 10_000).expect("a dump").to_text();

    let dir = std::env::temp_dir().join(format!("vimbai-swap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let on_disk = FileGraph::open(dir.join("store.json")).expect("a file store");

    Dump::from_text(&text)
        .expect("a dump")
        .restore(&on_disk)
        .expect("a restore");

    // read it back through a second handle, so this is the file talking, not a cache
    let reopened = FileGraph::open(dir.join("store.json")).expect("the store again");
    let entries = Records::new(&reopened, JOURNAL_ENTRY)
        .list(&scope, 100)
        .expect("a listing");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        Dump::take(&reopened, 10_000).expect("a dump").records.len(),
        Dump::take(&source, 10_000).expect("a dump").records.len()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_truncated_dump_is_refused_rather_than_half_loaded() {
    let source = MemoryGraph::new();
    a_written_book(&source);
    let text = Dump::take(&source, 10_000).expect("a dump").to_text();

    let mut lines: Vec<&str> = text.lines().collect();
    let last = lines.pop().expect("at least one line");
    let cut = format!("{}\n{}", lines.join("\n"), &last[..last.len() / 2]);
    assert!(
        Dump::from_text(&cut).is_err(),
        "half a line was accepted as a whole dump"
    );

    let empty = MemoryGraph::new();
    assert!(
        Dump::from_text("")
            .expect("an empty dump")
            .restore(&empty)
            .is_ok(),
        "an empty dump is a legitimate state: an empty store"
    );
}

#[test]
fn every_record_kind_is_in_the_dump_catalogue() {
    // a kind missing from KINDS is a kind a swap silently drops, so the list is checked against the source rather than
    // against anybody's memory
    let source = include_str!("../src/lib.rs");
    let mut declared = Vec::new();
    for line in source.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("pub const ") {
            if let Some((name, _)) = rest.split_once(':') {
                if rest.contains("RecordKind") {
                    declared.push(name.trim().to_string());
                }
            }
        }
    }
    assert!(
        declared.len() > 100,
        "the source scan found only {} kinds",
        declared.len()
    );
    let listed: Vec<String> = KINDS.iter().map(|kind| kind.label.to_string()).collect();
    for kind in KINDS {
        assert!(
            listed.iter().filter(|label| *label == kind.label).count() == 1,
            "{} is listed twice",
            kind.label
        );
    }
    assert_eq!(
        listed.len(),
        declared.len(),
        "KINDS holds {} kinds and the crate declares {}",
        listed.len(),
        declared.len()
    );
}
