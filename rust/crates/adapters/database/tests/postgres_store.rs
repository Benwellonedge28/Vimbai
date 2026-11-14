//! The deployment store, against a real Postgres.
//!
//! These tests need a server, and they are the reason P8.6 waited: a database adapter tested against a fake proves the
//! adapter agrees with the fake. They look for `VIMBAI_TEST_POSTGRES` and skip when it is absent, so a laptop with no
//! Postgres still runs the suite honestly instead of failing on something it was never given.
//!
//! Each test builds its own schema, in its own randomly named namespace, and drops it afterwards: one test's leftovers
//! must not decide another test's answer.

use serde_json::{json, Value};

use vimbai_store::postgres::PostgresGraph;
use vimbai_store::shared::SharedRecords;
use vimbai_store::{
    BookScope, Graph, PlatformRecords, RecordKind, Records, JOURNAL_ENTRY, JOURNAL_LINE,
    LEDGER_ACCOUNT,
};

/// A namespace of this test's own, dropped when the guard goes.
struct Database {
    dsn: String,
    schema: String,
}

impl Database {
    fn open() -> Option<Self> {
        let base = std::env::var("VIMBAI_TEST_POSTGRES").ok()?;
        let schema = format!("vimbai_test_{}", std::process::id()) + &format!("_{}", unique());
        let mut admin = postgres::Client::connect(&base, postgres::NoTls)
            .unwrap_or_else(|error| panic!("cannot reach {base}: {error}"));
        admin
            .batch_execute(&format!("CREATE SCHEMA {schema};"))
            .unwrap_or_else(|error| panic!("cannot create {schema}: {error}"));
        // search_path makes the store's own statements land in this namespace without the store having to know about it
        let dsn = format!("{base} options='-c search_path={schema}'");
        Some(Self { dsn, schema })
    }

    fn store(&self) -> PostgresGraph {
        PostgresGraph::connect(&self.dsn, 4).expect("a store")
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let base = self.dsn.split(" options=").next().unwrap().to_owned();
        if let Ok(mut admin) = postgres::Client::connect(&base, postgres::NoTls) {
            let _ = admin.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE;", self.schema));
        }
    }
}

fn unique() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(7)
}

fn scope(book: &str, user: &str) -> BookScope {
    BookScope {
        book_id: book.into(),
        user_id: user.into(),
    }
}

fn skip(name: &str) {
    eprintln!("{name}: skipped, VIMBAI_TEST_POSTGRES is not set");
}

#[test]
fn a_book_is_written_read_listed_and_kept_to_itself() {
    let Some(database) = Database::open() else {
        return skip("a_book_is_written_read_listed_and_kept_to_itself");
    };
    let store = database.store();
    let entries = Records::new(&store, JOURNAL_ENTRY);

    let entry = entries
        .create(
            &scope("book-a", "sam"),
            &json!({"description": "opening", "entry_date": "2026-11-14", "lines": []}),
        )
        .expect("an entry");
    let id = entry.get("id").and_then(Value::as_str).unwrap().to_owned();

    assert_eq!(
        entries.get(&scope("book-a", "sam"), &id).expect("a read")["description"],
        json!("opening")
    );
    // the same id in another Book is an absence, not somebody else's record
    assert!(entries.get(&scope("book-b", "sam"), &id).is_err());

    // and a listing is scoped too: the row is in one Book, so the other Book's list is empty
    entries
        .create(
            &scope("book-b", "tariro"),
            &json!({"description": "theirs"}),
        )
        .expect("their entry");
    let mine = entries.list(&scope("book-a", "sam"), 20).expect("a list");
    assert_eq!(mine.len(), 1, "a listing leaked another Book: {mine:?}");
}

#[test]
fn an_entry_and_its_lines_arrive_together_and_are_walked_from_the_account() {
    let Some(database) = Database::open() else {
        return skip("an_entry_and_its_lines_arrive_together_and_are_walked_from_the_account");
    };
    let store = database.store();
    let accounts = Records::new(&store, LEDGER_ACCOUNT);
    let book = scope("book-1", "sam");

    accounts
        .create(&book, &json!({"account_number": "1000", "name": "Cash"}))
        .expect("cash");
    accounts
        .create(&book, &json!({"account_number": "4000", "name": "Sales"}))
        .expect("sales");

    let entries = Records::new(&store, JOURNAL_ENTRY);
    let entry = entries
        .create_with_children(
            &book,
            &json!({"description": "a sale", "entry_date": "2026-11-14"}),
            JOURNAL_LINE,
            &[
                json!({"account_number": "1000", "side": "debit", "amount": "1000.00"}),
                json!({"account_number": "4000", "side": "credit", "amount": "1000.00"}),
            ],
            "entry_id",
        )
        .expect("a posting");

    let read = entries
        .get_with_children(
            &book,
            entry.get("id").and_then(Value::as_str).unwrap(),
            JOURNAL_LINE,
        )
        .expect("the entry back");
    assert_eq!(read["lines"].as_array().unwrap().len(), 2);

    let postings = entries
        .lines_for_account(&book, JOURNAL_LINE, "4000", 20)
        .expect("postings from the account");
    assert_eq!(postings.len(), 1);
    assert_eq!(postings[0]["entry_description"], json!("a sale"));
}

#[test]
fn a_refused_posting_leaves_no_edge_and_no_half_entry() {
    let Some(database) = Database::open() else {
        return skip("a_refused_posting_leaves_no_edge_and_no_half_entry");
    };
    let store = database.store();
    let book = scope("book-1", "sam");
    let entries = Records::new(&store, JOURNAL_ENTRY);

    // a line that is not a record is refused, and the entry that came with it must not stay behind
    let refused = entries.create_with_children(
        &book,
        &json!({"description": "half a posting", "entry_date": "2026-11-14"}),
        JOURNAL_LINE,
        &[json!("a line that is not a record")],
        "entry_id",
    );
    assert!(
        refused.is_err(),
        "a line with no id was stored: {refused:?}"
    );

    assert_eq!(entries.list(&book, 20).expect("a list").len(), 0);
    assert!(store.all_edges().expect("edges").is_empty());
}

#[test]
fn an_amount_of_thirty_nine_digits_survives_the_crossing() {
    let Some(database) = Database::open() else {
        return skip("an_amount_of_thirty_nine_digits_survives_the_crossing");
    };
    let store = database.store();
    let people = PlatformRecords::new(&store, RecordKind::new("Account", "OWNS_ACCOUNT"));
    let amount = "123456789012345678901234567890.123456789";
    let made = people
        .create(&json!({"id": "sam-1", "email": "sam@example.test", "balance": amount, "big": 39}))
        .expect("an account");

    let id = made.get("id").and_then(Value::as_str).unwrap().to_owned();
    let back = people.get(&id).expect("a read").expect("an account back");
    // exact, as text: a float would have rounded this at the sixteenth digit
    assert_eq!(back["balance"], json!(amount));
    assert_eq!(back["big"], json!(39));
}

#[test]
fn a_platform_record_is_stored_found_changed_and_removed() {
    let Some(database) = Database::open() else {
        return skip("a_platform_record_is_stored_found_changed_and_removed");
    };
    let store = database.store();
    let people = PlatformRecords::new(&store, RecordKind::new("Account", "OWNS_ACCOUNT"));

    people
        .create(&json!({"id": "sam-2", "email": "sam@example.test", "username": "sam"}))
        .expect("an account");
    assert!(people
        .find("email", "sam@example.test")
        .expect("a find")
        .is_some());

    let found = people
        .find("email", "sam@example.test")
        .expect("a find")
        .unwrap();
    let id = found.get("id").and_then(Value::as_str).unwrap().to_owned();

    let changed = people
        .update(&id, &json!({"email": "new@example.test"}))
        .expect("an update")
        .expect("the record");
    assert_eq!(changed["email"], json!("new@example.test"));
    // the rest of the record is untouched
    assert_eq!(changed["username"], json!("sam"));

    assert!(people.remove(&id).expect("a removal"));
    assert!(people.get(&id).expect("a read").is_none());
    assert!(!people.remove(&id).expect("a second removal"));
}

#[test]
fn a_platform_write_replaces_rather_than_colliding() {
    let Some(database) = Database::open() else {
        return skip("a_platform_write_replaces_rather_than_colliding");
    };
    let store = database.store();
    let people = PlatformRecords::new(&store, RecordKind::new("Account", "OWNS_ACCOUNT"));

    let first = people
        .create(&json!({"id": "fixed", "email": "sam@example.test"}))
        .expect("an account");
    assert_eq!(first["email"], json!("sam@example.test"));
    // the same write twice is what a resumed restore does, and it must not fail on the second attempt
    let again = people
        .create(&json!({"id": "fixed", "email": "changed@example.test"}))
        .expect("the same id again");
    assert_eq!(again["email"], json!("changed@example.test"));
    assert_eq!(people.list(10).expect("a list").len(), 1);
}

#[test]
fn a_row_shared_by_a_book_is_written_listed_found_and_removed() {
    let Some(database) = Database::open() else {
        return skip("a_row_shared_by_a_book_is_written_listed_found_and_removed");
    };
    let store = database.store();
    let memberships = SharedRecords::new(&store, RecordKind::new("Membership", "HAS_MEMBER"));

    let created = memberships
        .create("book-1", &json!({"member_user_id": "sam", "role": "owner"}))
        .expect("a membership");
    memberships
        .create(
            "book-1",
            &json!({"member_user_id": "tariro", "role": "member", "book_id": "book-9"}),
        )
        .expect("a second membership");

    // the Book that wrote the row owns it, not the Book the payload claimed
    assert_eq!(created["book_id"], json!("book-1"));
    assert_eq!(memberships.list("book-1", 10).expect("a list").len(), 2);
    assert_eq!(
        memberships.list("book-2", 10).expect("another Book").len(),
        0
    );

    let mine = memberships
        .where_eq("book-1", "member_user_id", &json!("sam"))
        .expect("a match");
    assert_eq!(mine.len(), 1);
    assert_eq!(
        memberships
            .everywhere("member_user_id", &json!("sam"))
            .expect("a wider read")
            .len(),
        1
    );

    let id = created
        .get("id")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    let changed = memberships
        .update("book-1", &id, &json!({"role": "admin"}))
        .expect("an update");
    assert_eq!(changed["role"], json!("admin"));

    memberships.delete("book-1", &id).expect("a removal");
    assert_eq!(memberships.list("book-1", 10).expect("a list").len(), 1);
    assert!(
        memberships.delete("book-1", &id).is_err(),
        "a second removal was accepted"
    );
}

#[test]
fn the_deployment_store_answers_operations_and_refuses_statements() {
    let Some(database) = Database::open() else {
        return skip("the_deployment_store_answers_operations_and_refuses_statements");
    };
    let store = database.store();
    // the whole point of the semantic doors: a Postgres backend is not a Cypher interpreter with a SQL accent
    let refused = store.run("MATCH (x:Book) RETURN x", json!({}));
    assert!(refused.is_err(), "a statement was answered: {refused:?}");
    assert!(
        format!("{}", refused.unwrap_err()).contains("operations, not statements"),
        "the refusal did not say why"
    );
}

#[test]
fn many_writers_on_one_book_lose_nothing() {
    let Some(database) = Database::open() else {
        return skip("many_writers_on_one_book_lose_nothing");
    };
    let store = std::sync::Arc::new(database.store());
    let book = scope("book-1", "sam");

    let writers = 24;
    let mut handles = Vec::new();
    for writer in 0..writers {
        let store = std::sync::Arc::clone(&store);
        let book = book.clone();
        handles.push(std::thread::spawn(move || {
            let accounts = Records::new(store.as_ref(), LEDGER_ACCOUNT);
            for index in 0..8 {
                accounts
                    .create(
                        &book,
                        &json!({
                            "account_number": format!("{writer}-{index}"),
                            "name": "concurrent",
                            "amount": "100000000000000000000000000000.000000001",
                        }),
                    )
                    .expect("a concurrent account");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("a writer finished");
    }

    let accounts = Records::new(store.as_ref(), LEDGER_ACCOUNT);
    let all = accounts.list(&book, 1000).expect("a list");
    assert_eq!(all.len(), writers * 8, "writes were lost");
    // every amount is exact and identical: a store that rounded would show a different tail on some rows
    for record in &all {
        assert_eq!(
            record["amount"],
            json!("100000000000000000000000000000.000000001")
        );
    }
}

#[test]
fn two_books_write_at_the_same_time_without_waiting_for_each_other() {
    let Some(database) = Database::open() else {
        return skip("two_books_write_at_the_same_time_without_waiting_for_each_other");
    };
    let store = std::sync::Arc::new(database.store());
    let mut handles = Vec::new();
    for book in 0..4 {
        let store = std::sync::Arc::clone(&store);
        handles.push(std::thread::spawn(move || {
            let entries = Records::new(store.as_ref(), JOURNAL_ENTRY);
            let scope = scope(&format!("book-{book}"), "sam");
            for index in 0..10 {
                entries
                    .create(
                        &scope,
                        &json!({"description": format!("entry {index}"), "book": book}),
                    )
                    .expect("an entry");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("a writer finished");
    }

    let entries = Records::new(store.as_ref(), JOURNAL_ENTRY);
    for book in 0..4 {
        let listed = entries
            .list(&scope(&format!("book-{book}"), "sam"), 100)
            .expect("a list");
        assert_eq!(listed.len(), 10, "book {book} holds {:?}", listed.len());
        for record in &listed {
            assert_eq!(record["book"], json!(book), "a record crossed Books");
        }
    }
}

#[test]
fn the_store_carries_a_book_between_sqlite_and_postgres() {
    let Some(database) = Database::open() else {
        return skip("the_store_carries_a_book_between_sqlite_and_postgres");
    };
    let deployment = database.store();
    let device = vimbai_store::sqlite::SqliteGraph::in_memory().expect("a device store");
    let book = scope("book-1", "sam");

    let accounts = Records::new(&device, LEDGER_ACCOUNT);
    let entries = Records::new(&device, JOURNAL_ENTRY);
    accounts
        .create(&book, &json!({"account_number": "1000", "name": "Cash"}))
        .expect("cash");
    entries
        .create_with_children(
            &book,
            &json!({"description": "moved", "entry_date": "2026-11-14"}),
            JOURNAL_LINE,
            &[json!({"account_number": "1000", "side": "debit", "amount": "123456789012345678901234567890.5"})],
            "entry_id",
        )
        .expect("a posting");

    let dump = vimbai_store::dump::Dump::take(&device, 1000).expect("a dump");
    let restored = dump.restore(&deployment).expect("a restore");
    assert!(
        restored.records >= 3,
        "not everything arrived: {restored:?}"
    );

    // the far side answers the same questions, including the walk from the account that a statement would have carried
    let moved = Records::new(&deployment, JOURNAL_ENTRY);
    let postings = moved
        .lines_for_account(&book, JOURNAL_LINE, "1000", 10)
        .expect("postings on the far side");
    assert_eq!(postings.len(), 1);
    assert_eq!(
        postings[0]["l"]["amount"],
        json!("123456789012345678901234567890.5")
    );
    assert_eq!(
        dump.differences(
            &vimbai_store::dump::Dump::take(&deployment, 1000).expect("a second dump")
        ),
        Vec::<String>::new(),
        "what arrived is not what left"
    );
}
