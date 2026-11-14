//! Many writers, one Book, at the same time.
//!
//! The engine serves one Book from one store, so two requests that arrive together are two writes that have to both land.
//! These tests are deliberately not about throughput: they are about whether a lost update is possible, and the answer has
//! to be no at a hundred writers as well as at two.

use std::sync::Arc;

use vimbai_common::Money;
use vimbai_scale::{big_amount, concurrent_writers, expense, total};
use vimbai_store::fake::MemoryGraph;
use vimbai_store::{amounts::stored_amount, BookScope, Records, EXPENSE};

#[test]
fn sixteen_writers_on_one_book_lose_nothing() {
    let graph = Arc::new(MemoryGraph::new());
    let run = concurrent_writers(&graph, "concurrent-book", 16, 25);
    assert_eq!(
        run.written,
        run.threads * run.per_thread,
        "entries went missing"
    );
}

#[test]
fn a_hundred_writers_on_one_book_lose_nothing() {
    // Past the number of workers a machine actually has, because the interesting interleavings start when threads
    // outnumber cores.
    let graph = Arc::new(MemoryGraph::new());
    let run = concurrent_writers(&graph, "wide-book", 100, 5);
    assert_eq!(run.written, 500, "entries went missing under contention");
}

#[test]
fn concurrent_writers_produce_an_exact_total() {
    // A count can be right while the values are wrong, so the amounts are summed too: one unit per entry, so the total
    // is a number a reader can check without a calculator.
    let graph = Arc::new(MemoryGraph::new());
    let run = concurrent_writers(&graph, "summed-book", 24, 10);
    let scope = BookScope::new("scale-user", "summed-book");
    let rows = Records::new(&*graph, EXPENSE)
        .list(&scope, run.threads * run.per_thread + 1)
        .expect("the entries");
    let amounts: Vec<Money> = rows
        .iter()
        .map(|row| stored_amount(row, "amount").expect("an amount"))
        .collect();
    let expected =
        Money::from_f64_shortest((run.threads * run.per_thread) as f64).expect("a total");
    assert_eq!(total(&amounts), expected);
}

#[test]
fn one_book_s_writes_are_invisible_to_another_book_under_concurrency() {
    // Scope is applied by the store, not by the caller, so this has to hold while two Books are being written at once -
    // otherwise the guarantee is "single threaded".
    let graph = Arc::new(MemoryGraph::new());
    let mut handles = Vec::new();
    for book in ["book-a", "book-b"] {
        let graph = Arc::clone(&graph);
        handles.push(std::thread::spawn(move || {
            let scope = BookScope::new("scale-user", book);
            for index in 0..50usize {
                expense(&*graph, &scope, &big_amount(30, index as u64 + 1), index)
                    .expect("a write");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("a writer panicked");
    }
    for book in ["book-a", "book-b"] {
        let scope = BookScope::new("scale-user", book);
        let rows = Records::new(&*graph, EXPENSE)
            .list(&scope, 100)
            .expect("the entries");
        assert_eq!(rows.len(), 50, "{book} saw the other book's writes");
        for row in &rows {
            assert_eq!(
                row["book_id"].as_str().expect("a book id"),
                book,
                "a record came back stamped with another book"
            );
        }
    }
}

#[test]
fn concurrent_writers_keep_their_thirty_digit_amounts_intact() {
    // Concurrency and magnitude together: the failure this catches is a value that survives the store when written alone
    // and quietly changes when a hundred writers are sharing the graph.
    let graph = Arc::new(MemoryGraph::new());
    let scope = BookScope::new("scale-user", "big-book");
    let expected: Vec<String> = (1..=32u64)
        .map(|offset| big_amount(30, offset).to_string())
        .collect();
    let mut handles = Vec::new();
    for (index, amount) in expected.iter().cloned().enumerate() {
        let graph = Arc::clone(&graph);
        let scope = scope.clone();
        handles.push(std::thread::spawn(move || {
            let value = Money::parse(&amount).expect("an amount");
            expense(&*graph, &scope, &value, index).expect("a write");
        }));
    }
    for handle in handles {
        handle.join().expect("a writer panicked");
    }
    let rows = Records::new(&*graph, EXPENSE)
        .list(&scope, 100)
        .expect("the entries");
    let mut stored: Vec<String> = rows
        .iter()
        .map(|row| {
            row["amount"]
                .as_str()
                .expect("an amount string")
                .to_string()
        })
        .collect();
    let mut wanted = expected.clone();
    stored.sort();
    wanted.sort();
    assert_eq!(stored, wanted, "a large amount changed under concurrency");
}
