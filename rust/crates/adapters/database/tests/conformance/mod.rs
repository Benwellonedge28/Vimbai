//! What every store must do to be a store Vimbai will run on.
//!
//! The trait is the swap boundary, so it is not enough that a provider compiles: a provider that answers a Book's
//! records in a different order, or lets one Book see another's, is a different system of record wearing the same trait.
//! This suite is the executable form of the portability claim, and every provider Vimbai offers runs it: the in-memory
//! graph, the file store, SQLite, and Postgres when a server is available.
//!
//! It deliberately tests the contract, not the dialect: nothing here names SQL, Cypher or a column. A provider is free to
//! be clever underneath as long as these answers come out the same.

use serde_json::{json, Value};
use vimbai_store::read::field;
use vimbai_store::{BookScope, Graph, StoreError, JOURNAL_ENTRY, LEDGER_ACCOUNT};

fn book_scope(book: &str) -> BookScope {
    BookScope::new("user-1", book)
}

/// A record as the engine writes one: the store keeps the id it is given rather than inventing one, because the id is
/// what a Book's ordering and its cursors are built on.
fn an_account(book: &str, id: &str, number: &str, name: &str) -> Value {
    json!({
        "id": id,
        "book_id": book,
        "user_id": "user-1",
        "account_number": number,
        "name": name,
        "account_type": "asset",
        "normal_balance": "debit",
    })
}

/// Every rule a store must keep, run against one open store.
pub fn every_rule(graph: &(impl Graph + ?Sized), book: &str) {
    let scope = book_scope(book);
    // a store Vimbai runs on says what it can do, and can hold a relationship: a Book whose records are not joined to
    // each other is not a Book Vimbai can move or dump faithfully
    let can = graph.capabilities();
    assert!(!can.name.is_empty(), "a store names itself in a refusal");
    assert!(
        can.edges,
        "{} must hold relationships between records",
        can.name
    );

    let elsewhere = book_scope(&format!("{book}-elsewhere"));

    // a record arrives as it was written, under the Book it was written in
    let written = graph
        .store_create(
            LEDGER_ACCOUNT,
            &scope,
            an_account(book, &format!("{book}-0001"), "1010", "Bank"),
        )
        .expect("a store accepts a record");
    let id = field(&written, "id").to_string();
    assert!(!id.is_empty(), "a stored record has an id");
    assert_eq!(field(&written, "account_number"), "1010");

    let read = graph
        .store_get(LEDGER_ACCOUNT, &scope, &id)
        .expect("a store answers a record it holds");
    assert_eq!(field(&read, "name"), "Bank");

    // a Book is a wall: another Book holds nothing, and cannot reach this record by id
    assert!(
        graph
            .store_list(LEDGER_ACCOUNT, &elsewhere, 10)
            .expect("a store answers an empty Book")
            .is_empty(),
        "another Book must not see this Book's records"
    );
    assert!(
        graph.store_get(LEDGER_ACCOUNT, &elsewhere, &id).is_err(),
        "another Book must not reach a record by its id"
    );

    // a kind is a wall too: an entry is not an account
    assert!(
        graph
            .store_list(JOURNAL_ENTRY, &scope, 10)
            .expect("a store answers a kind it holds nothing of")
            .is_empty(),
        "a record must not appear under another kind"
    );

    // an update changes what was named and leaves the rest
    let patched = graph
        .store_update(LEDGER_ACCOUNT, &scope, &id, json!({"name": "Bank account"}))
        .expect("a store updates a record it holds");
    assert_eq!(field(&patched, "name"), "Bank account");
    assert_eq!(
        field(&patched, "account_number"),
        "1010",
        "an update leaves what it did not name"
    );

    // a figure arrives to the last digit, as text: a store that rounds is a store that loses money
    let exact = graph
        .store_create(
            LEDGER_ACCOUNT,
            &scope,
            json!({
                "id": format!("{book}-0002"),
                "book_id": book,
                "user_id": "user-1",
                "account_number": "1011",
                "name": "Exact",
                "account_type": "asset",
                "normal_balance": "debit",
                "balance": "1234567890.123456",
            }),
        )
        .expect("a store accepts a figure");
    assert_eq!(
        field(&exact, "balance"),
        "1234567890.123456",
        "a store must not round a figure"
    );

    // a listing is in id order, newest first, and a page past a cursor neither skips a row nor repeats one
    let mut written_ids = vec![id.clone(), field(&exact, "id").to_string()];
    for (at, number) in ["2000", "2001", "2002", "2003"].iter().enumerate() {
        let row = graph
            .store_create(
                LEDGER_ACCOUNT,
                &scope,
                an_account(book, &format!("{book}-000{}", at + 3), number, "Row"),
            )
            .expect("a store accepts a record");
        written_ids.push(field(&row, "id").to_string());
    }
    let listed: Vec<String> = graph
        .store_list(LEDGER_ACCOUNT, &scope, 100)
        .expect("a store lists")
        .iter()
        .map(|row| field(row, "id").to_string())
        .collect();
    let mut expected = written_ids.clone();
    expected.sort();
    let mut seen = listed.clone();
    seen.sort();
    assert_eq!(
        seen, expected,
        "a listing holds every record of the Book, each once"
    );

    // a page from a place is in id order and starts after the place: an id never moves, so a client walking pages cannot
    // skip a record written while it was reading the last one
    let page = graph
        .store_list_after(LEDGER_ACCOUNT, &scope, &expected[1], 100)
        .expect("a store seeks past a place");
    let after: Vec<String> = page
        .iter()
        .map(|row| field(row, "id").to_string())
        .collect();
    assert_eq!(
        after,
        expected[2..].to_vec(),
        "a page past a place holds exactly the rows after it, in id order"
    );

    // deleting removes it, and deleting it again is refused rather than quietly fine
    graph
        .store_delete(LEDGER_ACCOUNT, &scope, &id)
        .expect("a store deletes a record it holds");
    assert!(
        graph.store_get(LEDGER_ACCOUNT, &scope, &id).is_err(),
        "a deleted record is gone"
    );
    assert!(
        graph.store_delete(LEDGER_ACCOUNT, &scope, &id).is_err(),
        "deleting what is not there is refused"
    );
}

/// The same rule for edges: a store may decline to hold them, it may not pretend it does.
pub fn edges_are_data_or_refused(
    graph: &(impl Graph + ?Sized),
    book: &str,
) -> Result<(), StoreError> {
    let scope = book_scope(book);
    let from = graph.store_create(
        LEDGER_ACCOUNT,
        &scope,
        an_account(book, &format!("{book}-0001"), "3000", "One"),
    )?;
    let to = graph.store_create(
        LEDGER_ACCOUNT,
        &scope,
        an_account(book, &format!("{book}-0002"), "3001", "Two"),
    )?;
    graph.create_edge(field(&from, "id"), "DEBITED", field(&to, "id"))?;
    let edges = graph.all_edges()?;
    assert!(
        edges.iter().any(|(a, label, b)| label == "DEBITED"
            && a == field(&from, "id")
            && b == field(&to, "id")),
        "an edge written is an edge reported"
    );
    Ok(())
}
