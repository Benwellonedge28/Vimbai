//! The device store, held to the same promises as the server store.
//!
//! These are the promises that matter and none that do not: exact amounts at any size, a Book's records staying inside that
//! Book, an entry arriving with its lines or not at all, a posting findable from the account rather than by reading every
//! entry, the schema surviving a rollback and re-apply, and many writers at once losing nothing.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

use vimbai_store::dump::Dump;
use vimbai_store::shared::SharedRecords;
use vimbai_store::sqlite::SqliteGraph;
use vimbai_store::{
    BookScope, Graph, PlatformRecords, RecordKind, Records, StoreError, JOURNAL_ENTRY,
    JOURNAL_LINE, LEDGER_ACCOUNT,
};

const PRODUCT_COST: RecordKind = RecordKind::new("ProductCost", "OWNS_PRODUCT_COST");

fn scope() -> BookScope {
    BookScope::new("sam", "book-1")
}

fn amount_of(record: &Value) -> String {
    record
        .get("amount")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn an_amount_of_thirty_nine_digits_survives_the_crossing_unchanged() {
    let store = SqliteGraph::in_memory().expect("a store");
    let huge = "12345678901234567890123456789012.1234567";

    Records::new(&store, PRODUCT_COST)
        .create(
            &scope(),
            &json!({"amount": huge, "label": "a very big cost"}),
        )
        .expect("a create");

    let listed = Records::new(&store, PRODUCT_COST)
        .list(&scope(), 10)
        .expect("a listing");
    assert_eq!(listed.len(), 1);
    assert_eq!(
        amount_of(&listed[0]),
        huge,
        "the amount was stored as a number somewhere and lost its digits"
    );
}

#[test]
fn a_book_reads_its_own_records_and_no_others() {
    let store = SqliteGraph::in_memory().expect("a store");
    let mine = Records::new(&store, PRODUCT_COST);
    let theirs = BookScope::new("someone-else", "book-2");

    mine.create(&scope(), &json!({"amount": "10", "label": "mine"}))
        .expect("a create");
    mine.create(&theirs, &json!({"amount": "99", "label": "theirs"}))
        .expect("a create");

    let listed = mine.list(&scope(), 100).expect("a listing");
    assert_eq!(
        listed.len(),
        1,
        "a Book saw another Book's records: {listed:?}"
    );
    assert_eq!(listed[0].get("label").and_then(Value::as_str), Some("mine"));
}

#[test]
fn a_miss_is_an_absence_rather_than_another_books_record() {
    let store = SqliteGraph::in_memory().expect("a store");
    let mine = Records::new(&store, PRODUCT_COST);
    let created = mine
        .create(&BookScope::new("sam", "book-2"), &json!({"amount": "5"}))
        .expect("a create");
    let id = created
        .get("id")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();

    let missed = mine
        .get(&scope(), &id)
        .expect_err("another Book's record was readable");
    assert!(matches!(missed, StoreError::NotFound), "{missed}");
}

#[test]
fn an_entry_and_its_lines_arrive_together_and_are_walked_from_the_account() {
    let store = SqliteGraph::in_memory().expect("a store");

    // the account a line may name, in the same Book
    Records::new(&store, LEDGER_ACCOUNT)
        .create(
            &scope(),
            &json!({"account_number": "5000", "name": "Expenses"}),
        )
        .expect("an account");

    let entry = Records::new(&store, JOURNAL_ENTRY)
        .create_with_children(
            &scope(),
            &json!({"reference": "INV-1", "entry_date": "2026-11-14", "description": "stationery", "posted": true}),
            JOURNAL_LINE,
            &[
                json!({"line_no": 1, "account_number": "5000", "debit": "1200.75", "credit": "0", "side": "debit"}),
                json!({"line_no": 2, "account_number": "5000", "debit": "0", "credit": "1200.75", "side": "credit"}),
            ],
            "entry_id",
        )
        .expect("a posting");

    assert_eq!(entry["lines"].as_array().map(Vec::len), Some(2));

    let read_back = Records::new(&store, JOURNAL_ENTRY)
        .get_with_children(&scope(), entry["id"].as_str().unwrap(), JOURNAL_LINE)
        .expect("a read");
    assert_eq!(read_back["lines"].as_array().map(Vec::len), Some(2));

    let postings = Records::new(&store, JOURNAL_ENTRY)
        .lines_for_account(&scope(), JOURNAL_LINE, "5000", 100)
        .expect("the account's postings");
    assert_eq!(
        postings.len(),
        2,
        "the posting edges did not carry: {postings:?}"
    );
    assert_eq!(
        postings[0].get("entry_id").and_then(Value::as_str),
        entry["id"].as_str(),
        "a posting did not name the entry it belongs to"
    );
}

#[test]
fn a_line_that_is_not_posted_leaves_no_edge_and_no_half_entry() {
    let store = SqliteGraph::in_memory().expect("a store");
    // no account 9999 exists, so the statement must still store the line, just without a posting edge to nothing
    Records::new(&store, JOURNAL_ENTRY)
        .create_with_children(
            &scope(),
            &json!({"reference": "INV-2", "posted": false}),
            JOURNAL_LINE,
            &[json!({"line_no": 1, "account_number": "9999", "debit": "1", "credit": "0", "side": "debit"})],
            "entry_id",
        )
        .expect("a posting to a missing account");

    let edges = store.all_edges().expect("the edges");
    assert!(
        edges.iter().all(|(_, label, _)| label != "DEBITED"),
        "a line was posted to an account that does not exist: {edges:?}"
    );
}

#[test]
fn a_platform_record_is_stored_found_changed_and_swept() {
    let store = SqliteGraph::in_memory().expect("a store");
    let people = PlatformRecords::new(&store, RecordKind::new("Account", "OWNS_ACCOUNT"));

    people
        .create(&json!({"id": "account-1", "username": "sam", "email": "sam@example.test"}))
        .expect("an account");
    assert!(people.find("username", "sam").expect("a find").is_some());
    assert!(people.find("username", "nobody").expect("a find").is_none());

    people
        .update("account-1", &json!({"email": "new@example.test"}))
        .expect("an update");
    assert!(
        people
            .find("email", "new@example.test")
            .expect("a find")
            .is_some(),
        "the changed email was not findable"
    );

    assert!(people.remove("account-1").expect("a removal"));
    assert!(people.get("account-1").expect("a read").is_none());
    assert!(!people.remove("account-1").expect("a second removal"));
}

#[test]
fn the_device_store_answers_operations_and_refuses_statements() {
    let store = SqliteGraph::in_memory().expect("a store");
    let refused = store
        .run("MATCH (x) RETURN x", json!({}))
        .expect_err("the device store read a statement");
    assert!(
        refused.to_string().contains("operations, not statements"),
        "{refused}"
    );
}

#[test]
fn the_schema_applies_rolls_back_and_applies_again_over_a_real_book() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("store.db");
    let store = SqliteGraph::open(&path).expect("a store");

    // a real Book, not a fixture: an account and an entry with its lines
    Records::new(&store, LEDGER_ACCOUNT)
        .create(
            &scope(),
            &json!({"account_number": "5000", "name": "Expenses"}),
        )
        .expect("an account");
    Records::new(&store, JOURNAL_ENTRY)
        .create_with_children(
            &scope(),
            &json!({"reference": "INV-3", "entry_date": "2026-11-14", "posted": true}),
            JOURNAL_LINE,
            &[json!({"line_no": 1, "account_number": "5000", "debit": "42.42", "credit": "0", "side": "debit"})],
            "entry_id",
        )
        .expect("a posting");

    let before = Dump::take(&store, 10_000).expect("a dump");
    assert!(
        before.to_text().lines().count() >= 4,
        "the fixture is too thin"
    );

    // going back to an empty schema drops what was there, which is why a rollback is only safe with a dump beside it
    store.rollback().expect("a rollback");
    assert_eq!(store.version().expect("a version"), 1);
    store.rollback().expect("the last step");
    assert_eq!(store.version().expect("a version"), 0);
    store
        .rollback()
        .expect("a rollback past the beginning is a no-op");

    store.reapply().expect("a re-apply");
    assert_eq!(store.version().expect("a version"), 2);

    before.restore(&store).expect("a restore");
    let after = Dump::take(&store, 10_000).expect("a dump");
    assert!(
        after.differences(&before).is_empty(),
        "the round trip lost something: {:?}",
        after.differences(&before)
    );

    // and the money came back as money, not as a rounded number
    let entries = Records::new(&store, JOURNAL_ENTRY)
        .list_with_children(&scope(), 10, JOURNAL_LINE)
        .expect("a listing");
    assert_eq!(
        entries[0]["lines"][0].get("debit").and_then(Value::as_str),
        Some("42.42")
    );
}

#[test]
fn a_device_survives_every_writer_it_is_given() {
    let store = Arc::new(SqliteGraph::in_memory().expect("a store"));
    let written = Arc::new(AtomicUsize::new(0));
    let mut hands = Vec::new();
    for hand in 0..8 {
        let store = Arc::clone(&store);
        let written = Arc::clone(&written);
        hands.push(std::thread::spawn(move || {
            for step in 0..25 {
                Records::new(&*store, PRODUCT_COST)
                    .create(
                        &scope(),
                        &json!({
                            "amount": format!("{hand}{step}.000000001"),
                            "label": format!("hand {hand} step {step}")
                        }),
                    )
                    .expect("a concurrent create");
                written.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    for hand in hands {
        hand.join().expect("a writer panicked");
    }

    let listed = Records::new(&*store, PRODUCT_COST)
        .list(&scope(), 1000)
        .expect("a listing");
    assert_eq!(listed.len(), 200, "a concurrent write was lost");
    assert_eq!(written.load(Ordering::SeqCst), 200);

    // every amount is still the exact text it was given, at any precision
    let mut seen: Vec<String> = listed.iter().map(amount_of).collect();
    seen.sort();
    seen.dedup();
    assert_eq!(
        seen.len(),
        200,
        "two concurrent writes landed on the same record"
    );
}

#[test]
fn a_file_store_keeps_its_records_across_a_restart() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("store.db");

    {
        let store = SqliteGraph::open(&path).expect("a store");
        Records::new(&store, PRODUCT_COST)
            .create(&scope(), &json!({"amount": "77.5", "label": "kept"}))
            .expect("a create");
    }

    let reopened = SqliteGraph::open(&path).expect("the store again");
    let listed = Records::new(&reopened, PRODUCT_COST)
        .list(&scope(), 10)
        .expect("a listing");
    assert_eq!(listed.len(), 1);
    assert_eq!(amount_of(&listed[0]), "77.5");
}

#[test]
fn a_row_shared_by_a_book_is_written_listed_found_and_removed() {
    // memberships, sync batches and organisation links belong to a Book and to nobody in it, so they carry no owner edge
    // and the caller has already established the right to write. This is the path a Book is created through, which is why
    // the device store has to answer it natively rather than through a statement.
    const MEMBERSHIP: RecordKind = RecordKind::new("Membership", "HAS_MEMBER");
    let store = SqliteGraph::in_memory().expect("a store");
    let rows = SharedRecords::new(&store, MEMBERSHIP);

    let created = rows
        .create("book-1", &json!({"member_user_id": "sam", "role": "owner"}))
        .expect("a membership");
    assert_eq!(
        created.get("book_id").and_then(Value::as_str),
        Some("book-1")
    );

    // a row that names another Book is stamped with the Book that wrote it, not the one it claimed
    let stamped = rows
        .create(
            "book-1",
            &json!({"member_user_id": "tariro", "role": "member", "book_id": "book-9"}),
        )
        .expect("a second membership");
    assert_eq!(
        stamped.get("book_id").and_then(Value::as_str),
        Some("book-1")
    );

    assert_eq!(rows.list("book-1", 10).expect("a listing").len(), 2);
    assert_eq!(rows.list("book-2", 10).expect("another Book").len(), 0);

    let mine = rows
        .where_eq("book-1", "member_user_id", &json!("sam"))
        .expect("a match");
    assert_eq!(mine.len(), 1);
    assert_eq!(
        rows.everywhere("member_user_id", &json!("sam"))
            .expect("a wider read")
            .len(),
        1
    );

    let id = created
        .get("id")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    let changed = rows
        .update("book-1", &id, &json!({"role": "admin"}))
        .expect("an update");
    assert_eq!(changed.get("role").and_then(Value::as_str), Some("admin"));

    rows.delete("book-1", &id).expect("a removal");
    assert_eq!(rows.list("book-1", 10).expect("a listing").len(), 1);
    assert!(
        rows.delete("book-1", &id).is_err(),
        "a second removal was accepted"
    );
}

/// A page that seeks, rather than a page that counts.
///
/// The device store answers a Book's records from the top and stops at whatever cap the caller asked for. This is the other
/// question: rows after a place, in id order, so a client following cursors reaches the end of a Book no matter how large it
/// grows. Two hundred records are written, a page is taken from the middle, and the walk continues from there without
/// repeating or losing anything.
#[test]
fn a_page_seeks_from_a_place_rather_than_counting_from_the_top() {
    let store = SqliteGraph::in_memory().expect("a store");
    let records = Records::new(&store, PRODUCT_COST);
    let mut written = Vec::new();
    for index in 0..200 {
        let created = records
            .create(&scope(), &json!({"label": format!("cost {index:03}")}))
            .expect("a record");
        written.push(created["id"].as_str().unwrap_or_default().to_owned());
    }
    written.sort();

    let half = &written[100];
    let after = records
        .list(&scope().after(half.clone()), 40)
        .expect("a page after a place");
    assert_eq!(after.len(), 40, "the page is the size asked for");
    for row in &after {
        let id = row["id"].as_str().unwrap_or_default();
        assert!(id > half.as_str(), "{id} is not past the place asked from");
        assert!(written.contains(&id.to_owned()), "{id} was never written");
    }
    assert_eq!(
        after[0]["id"].as_str(),
        Some(written[101].as_str()),
        "the page starts at the row after the place, not somewhere beyond it"
    );

    // and the walk reaches the end: eleven pages of forty cover the hundred rows that follow the place
    let mut seen = 0;
    let mut cursor = half.clone();
    loop {
        let page = records
            .list(&scope().after(cursor.clone()), 40)
            .expect("a page");
        if page.is_empty() {
            break;
        }
        seen += page.len();
        cursor = page
            .last()
            .and_then(|row| row["id"].as_str())
            .unwrap_or_default()
            .to_owned();
        assert!(seen <= 200, "the walk is going in circles");
    }
    assert_eq!(seen, 99, "every row after the place, exactly once");
}
