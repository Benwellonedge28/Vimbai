#![forbid(unsafe_code)]
//! The rows a Book holds, on the store an on-device install boots.
//!
//! A Book's rows belong to the Book, not to whoever wrote them: a membership names the member, a metadata row names the person
//! a key was wrapped for, an audit line names who acted. Reading one back by that name is how an invitation is accepted and how
//! a member's Book is found - so the store has to answer "the Book's rows whose member_user_id is this person" without the caller
//! being the person the statement's other half names.
//!
//! This is the file store specifically because it is the one on a phone: the engine's database is covered elsewhere, and a
//! difference between them is a feature that works for the cluster and fails on the device.

use serde_json::json;
use vimbai_store::file::FileGraph;
use vimbai_store::shared::SharedRecords;
use vimbai_store::{RecordKind, StoreError};

/// The row type the sharing story is written in: a membership names the member, and the Book holds it.
fn membership() -> RecordKind {
    RecordKind::new("SyncMembership", "OWNS_SYNC_MEMBERSHIP")
}

fn held<'a>(graph: &'a FileGraph, kind: RecordKind) -> SharedRecords<'a, FileGraph> {
    SharedRecords::new(graph, kind)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("vimbai-shared-rows-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join("book.json")
}

#[test]
fn a_books_row_is_read_back_by_the_name_it_carries() -> Result<(), StoreError> {
    let path = scratch("by-name");
    {
        let graph = FileGraph::open(&path)?;
        held(&graph, membership()).create(
            "book-1",
            &json!({"id": "member-1", "member_user_id": "guest-1", "role": "bookkeeper", "status": "invited"}),
        )?;
    }

    // the row outlives the process that wrote it, which is what a phone needs
    let reopened = FileGraph::open(&path)?;
    let found =
        held(&reopened, membership()).where_eq("book-1", "member_user_id", &json!("guest-1"))?;
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["status"], json!("invited"));
    Ok(())
}

#[test]
fn one_persons_row_is_visible_to_the_book_and_not_to_another_book() -> Result<(), StoreError> {
    let path = scratch("scoping");
    let graph = FileGraph::open(&path)?;
    held(&graph, membership()).create(
        "book-1",
        &json!({"id": "member-1", "member_user_id": "guest-1", "role": "bookkeeper", "status": "invited"}),
    )?;
    held(&graph, membership()).create(
        "book-2",
        &json!({"id": "member-2", "member_user_id": "guest-1", "role": "owner", "status": "active"}),
    )?;

    // the invitation for book-1 is not answered by the membership of book-2
    let in_one =
        held(&graph, membership()).where_eq("book-1", "member_user_id", &json!("guest-1"))?;
    assert_eq!(in_one.len(), 1, "{in_one:?}");
    assert_eq!(in_one[0]["id"], json!("member-1"), "{in_one:?}");

    // and the Books a person belongs to are found across Books, which is how the app lists them
    let mine = held(&graph, membership()).everywhere("member_user_id", &json!("guest-1"))?;
    assert_eq!(mine.len(), 2, "{mine:?}");
    Ok(())
}

#[test]
fn a_row_is_read_back_after_the_store_is_written_out() -> Result<(), StoreError> {
    // every write replaces the whole file, so a read that only worked before the write-out would pass in memory and lose the
    // row on a phone
    let path = scratch("written-out");
    let graph = FileGraph::open(&path)?;
    held(&graph, membership()).create(
        "book-1",
        &json!({"id": "member-1", "member_user_id": "guest-1", "role": "bookkeeper", "status": "invited"}),
    )?;
    held(&graph, membership()).update("book-1", "member-1", &json!({"status": "active"}))?;

    let on_disk = std::fs::read_to_string(&path).expect("the store wrote itself out");
    assert!(
        on_disk.contains("member-1"),
        "the row reached the file: {on_disk}"
    );

    let reopened = FileGraph::open(&path)?;
    let found = held(&reopened, membership()).get("book-1", "member-1")?;
    assert_eq!(found["status"], json!("active"), "{found}");
    Ok(())
}
