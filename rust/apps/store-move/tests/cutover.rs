#![forbid(unsafe_code)]
//! What a move says before it happens, and what it leaves behind afterwards.
//!
//! A move is a decision about a business's books. The operator is told what it would carry and whether the destination is
//! empty first, is refused rather than allowed to mix two books, and afterwards holds a receipt that can be read instead
//! of remembered.

use serde_json::json;
use vimbai_store::file::FileGraph;
use vimbai_store::{BookScope, Graph, LEDGER_ACCOUNT};
use vimbai_store_move::{cutover, move_guarded};

fn place(name: &str) -> String {
    let directory =
        std::env::temp_dir().join(format!("vimbai-cutover-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a place to work in");
    format!("file:{}", directory.join("store.json").display())
}

fn an_account(number: &str) -> serde_json::Value {
    json!({
        "id": format!("acc-{number}"),
        "book_id": "book-1",
        "user_id": "user-1",
        "account_number": number,
        "name": "Bank",
        "account_type": "asset",
    })
}

fn seed(store: &str, numbers: &[&str]) {
    let path = store.trim_start_matches("file:");
    let graph = FileGraph::open(path).expect("a store to seed");
    let scope = BookScope::new("user-1", "book-1");
    for number in numbers {
        graph
            .store_create(LEDGER_ACCOUNT, &scope, an_account(number))
            .expect("a record");
    }
}

#[test]
fn a_plan_says_what_would_move_and_whether_the_destination_is_empty() {
    let from = place("plan-from");
    let to = place("plan-to");
    seed(&from, &["1010", "1020", "1030"]);

    let ahead = cutover(&from, &to, 1000).expect("a plan");
    assert_eq!(ahead.records, 3);
    assert!(
        ahead.destination_empty,
        "nothing has been written there yet"
    );
    assert_eq!(ahead.by_kind, vec![("Account".to_string(), 3)]);
    let said = ahead.to_text();
    assert!(said.contains("3 records"), "{said}");
    assert!(said.contains("empty"), "{said}");
    assert!(said.contains("Account"), "{said}");
}

#[test]
fn a_move_into_a_store_that_already_holds_books_is_refused_in_words() {
    let from = place("occupied-from");
    let to = place("occupied-to");
    seed(&from, &["1010"]);
    seed(&to, &["9999"]);

    let refused = move_guarded(&from, &to, 1000, None, false, false)
        .expect_err("an occupied destination is refused");
    let said = refused.to_string();
    assert!(said.contains("mixes two books"), "{said}");
    assert!(said.contains("--into-occupied"), "{said}");
}

#[test]
fn a_move_leaves_a_receipt_that_names_both_stores_and_the_verdict() {
    let from = place("receipt-from");
    let to = place("receipt-to");
    seed(&from, &["1010", "1020"]);

    let receipt = move_guarded(&from, &to, 1000, None, false, false).expect("a move");
    assert!(receipt.faithful(), "{:?}", receipt.differences);
    assert_eq!(receipt.records, 2);
    let said = receipt.to_text();
    assert!(said.contains(&from), "{said}");
    assert!(said.contains(&to), "{said}");
    assert!(said.starts_with("move cutover "), "{said}");
    assert!(
        said.contains("holds what the source held"),
        "the verdict is stated in words: {said}"
    );
}

#[test]
fn a_rehearsal_says_so_on_its_receipt() {
    let from = place("rehearsal-from");
    let to = place("rehearsal-to");
    seed(&from, &["1010"]);

    let receipt = move_guarded(&from, &to, 1000, None, false, true).expect("a rehearsal");
    let said = receipt.to_text();
    // the first line says which it was, so a receipt read months later cannot be mistaken for the other kind
    assert!(
        said.starts_with("move rehearsal "),
        "a rehearsal says so: {said}"
    );
}
