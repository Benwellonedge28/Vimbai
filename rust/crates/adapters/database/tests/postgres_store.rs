#![forbid(unsafe_code)]
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

const PRODUCT_COST: RecordKind = RecordKind::new("ProductCost", "OWNS_PRODUCT_COST");
use vimbai_store::shared::SharedRecords;
use vimbai_store::{
    BookScope, Graph, PlatformRecords, RecordKind, Records, JOURNAL_ENTRY, JOURNAL_LINE,
    LEDGER_ACCOUNT,
};
use vimbai_test_support::TestSchema;

/// A namespace of this test's own, from the store crate's one helper, so every suite names and drops a namespace the
/// same way and the create is retried when the server is busy with the other test binaries starting beside it.
fn database() -> Option<TestSchema> {
    TestSchema::open("vimbai_test")
}

fn scope(book: &str, user: &str) -> BookScope {
    BookScope {
        book_id: book.into(),
        user_id: user.into(),
        after: None,
    }
}

fn skip(name: &str) {
    eprintln!("{name}: skipped, VIMBAI_TEST_POSTGRES is not set");
}

#[test]
fn a_book_is_written_read_listed_and_kept_to_itself() {
    let Some(database) = database() else {
        return skip("a_book_is_written_read_listed_and_kept_to_itself");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
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
    let Some(database) = database() else {
        return skip("an_entry_and_its_lines_arrive_together_and_are_walked_from_the_account");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
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
    let Some(database) = database() else {
        return skip("a_refused_posting_leaves_no_edge_and_no_half_entry");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
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
    let Some(database) = database() else {
        return skip("an_amount_of_thirty_nine_digits_survives_the_crossing");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
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
    let Some(database) = database() else {
        return skip("a_platform_record_is_stored_found_changed_and_removed");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
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
    let Some(database) = database() else {
        return skip("a_platform_write_replaces_rather_than_colliding");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
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
    let Some(database) = database() else {
        return skip("a_row_shared_by_a_book_is_written_listed_found_and_removed");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
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
    let Some(database) = database() else {
        return skip("the_deployment_store_answers_operations_and_refuses_statements");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
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
    let Some(database) = database() else {
        return skip("many_writers_on_one_book_lose_nothing");
    };
    let store = std::sync::Arc::new(PostgresGraph::connect(database.dsn(), 4).expect("a store"));
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
    let Some(database) = database() else {
        return skip("two_books_write_at_the_same_time_without_waiting_for_each_other");
    };
    let store = std::sync::Arc::new(PostgresGraph::connect(database.dsn(), 4).expect("a store"));
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
    let Some(database) = database() else {
        return skip("the_store_carries_a_book_between_sqlite_and_postgres");
    };
    let deployment = PostgresGraph::connect(database.dsn(), 4).expect("a store");
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

/// A page that seeks, on the deployment store, where the database does the walking.
///
/// The same promise as the device store: rows after a place in id order, so a Book that outgrows a handler's cap is still
/// walkable. Here the seeking is `props ->> 'id' > $3` and an index on the row's own identity, not a page read into memory
/// and filtered afterwards.
#[test]
fn a_page_seeks_from_a_place_rather_than_counting_from_the_top() {
    let Some(database) = database() else {
        eprintln!("skipped: VIMBAI_TEST_POSTGRES is not set");
        return;
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
    let records = Records::new(&store, PRODUCT_COST);
    let mut written = Vec::new();
    for index in 0..60 {
        let created = records
            .create(
                &scope("book-1", "sam"),
                &json!({"label": format!("cost {index:03}")}),
            )
            .expect("a record");
        written.push(created["id"].as_str().unwrap_or_default().to_owned());
    }
    written.sort();

    let half = &written[30];
    let after = records
        .list(&scope("book-1", "sam").after(half.clone()), 20)
        .expect("a page after a place");
    assert_eq!(after.len(), 20);
    assert_eq!(
        after[0]["id"].as_str(),
        Some(written[31].as_str()),
        "the page starts at the row after the place"
    );
    for row in &after {
        let id = row["id"].as_str().unwrap_or_default();
        assert!(id > half.as_str(), "{id} is not past the place asked from");
    }
}

#[test]
fn the_schema_is_a_versioned_step_list_and_a_second_open_runs_nothing() {
    let Some(database) = database() else {
        return skip("the_schema_is_a_versioned_step_list_and_a_second_open_runs_nothing");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
    assert_eq!(
        store.version().expect("a version"),
        vimbai_store::postgres::SCHEMA.len() as i64,
        "a fresh database has to be at the version this build knows"
    );
    // a record written before the second open has to still be there afterwards: applying steps again is not re-creation
    let entries = Records::new(&store, JOURNAL_ENTRY);
    let entry = entries
        .create(
            &scope("book-a", "sam"),
            &json!({"description": "written before"}),
        )
        .expect("an entry");
    let id = entry.get("id").and_then(Value::as_str).unwrap().to_owned();
    let again = PostgresGraph::connect(database.dsn(), 4).expect("a store");
    assert_eq!(
        again.version().expect("a version"),
        vimbai_store::postgres::SCHEMA.len() as i64,
        "opening an up-to-date store changed its version"
    );
    let reopened = Records::new(&again, JOURNAL_ENTRY);
    assert_eq!(
        reopened.get(&scope("book-a", "sam"), &id).expect("a read")["description"],
        json!("written before"),
        "the record written before the second open is gone, so the schema was re-created rather than migrated"
    );
}

#[test]
fn a_store_written_by_a_newer_build_is_refused_rather_than_downgraded() {
    let Some(database) = database() else {
        return skip("a_store_written_by_a_newer_build_is_refused_rather_than_downgraded");
    };
    PostgresGraph::connect(database.dsn(), 4).expect("a store");
    let mut admin = postgres::Client::connect(database.dsn(), postgres::NoTls).expect("a client");
    admin
        .batch_execute("UPDATE schema_version SET version = 99;")
        .expect("a version to set");
    let refusal = match vimbai_store::postgres::PostgresGraph::connect(database.dsn(), 2) {
        Ok(_) => {
            panic!("a database claiming schema 99 was opened by a build that knows fewer steps")
        }
        Err(error) => format!("{error}"),
    };
    assert!(
        refusal.contains("newer build") && refusal.contains("Upgrade the engine"),
        "the refusal has to say what happened and what to do: {refusal}"
    );
}

#[test]
fn every_step_can_be_undone_and_re_applied_over_records_that_are_already_there() {
    let Some(database) = database() else {
        return skip("every_step_can_be_undone_and_re_applied_over_records_that_are_already_there");
    };
    let store = PostgresGraph::connect(database.dsn(), 4).expect("a store");
    Records::new(&store, JOURNAL_ENTRY)
        .create(&scope("book-a", "sam"), &json!({"description": "before"}))
        .expect("an entry");
    // walking all the way down and back up is what proves the `down` halves are real SQL rather than a comment
    let steps = vimbai_store::postgres::SCHEMA.len() as i64;
    for expected in (0..steps).rev() {
        store.rollback().expect("a rollback");
        assert_eq!(store.version().expect("a version"), expected);
    }
    assert_eq!(store.version().expect("a version"), 0);
    // re-applying finds an empty schema: nothing is left to hold the record, which is what an undone step means
    PostgresGraph::connect(database.dsn(), 4).expect("a store");
    let fresh = PostgresGraph::connect(database.dsn(), 4).expect("a store");
    assert_eq!(fresh.version().expect("a version"), steps);
    let listing = Records::new(&fresh, JOURNAL_ENTRY)
        .list(&scope("book-a", "sam"), 20)
        .expect("a list");
    assert!(
        listing.is_empty(),
        "the step's `down` said the table goes, so the record cannot survive a full rollback and re-apply: {listing:?}"
    );
}
