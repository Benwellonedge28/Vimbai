//! The scale suite's own helpers: exact totals over a stream of amounts, and many writers on one Book.
//!
//! These live in a library rather than in a test file because two things need them: the tests beside this crate, and any
//! later suite that wants to hammer the store instead of the arithmetic. A helper written inside one `#[test]` is a
//! helper the next test copies, and two copies of a summing rule is how a report disagrees with a ledger.
//!
//! Nothing here uses a float. An amount that fits in an `f64` to the cent at twenty digits does not fit at forty, and the
//! whole point of these tests is that the platform never has to care which one it was given.
#![forbid(unsafe_code)]

use std::sync::Arc;

use vimbai_common::Money;
use vimbai_store::fake::MemoryGraph;
use vimbai_store::{BookScope, Graph, Records, StoreError, EXPENSE};

/// The exact total of a stream of amounts, in the order given.
///
/// `Money::sum` is the one summing rule in the workspace and this calls it rather than folding a second time: a second
/// fold is a second rounding order waiting to disagree with the first.
pub fn total<'a, I: IntoIterator<Item = &'a Money>>(amounts: I) -> Money {
    Money::sum(amounts)
}

/// An amount with `digits` integer digits, offset so every one in a run is distinct.
///
/// Built as text and parsed exactly, because an `f64` stops being able to hold an integer at about sixteen digits and
/// every digit past that is the test lying to itself.
pub fn big_amount(digits: usize, offset: u64) -> Money {
    let body = format!("{}{:0>width$}", 1, offset, width = digits - 1);
    let text = body[..digits].to_string();
    Money::parse(&text).expect("a decimal this function wrote itself")
}

/// One expense in a Book, holding an exact amount as a decimal string.
pub fn expense(
    graph: &impl Graph,
    scope: &BookScope,
    amount: &Money,
    sequence: usize,
) -> Result<(), StoreError> {
    Records::new(graph, EXPENSE).create(
        scope,
        &serde_json::json!({
            "description": format!("entry {sequence}"),
            "amount": amount.to_string(),
            "sequence": sequence,
        }),
    )?;
    Ok(())
}

/// What a run of `write` produced: how many writers ran, and how many entries the Book ended up with.
pub struct Writers {
    pub threads: usize,
    pub per_thread: usize,
    pub written: usize,
}

/// Run `threads` writers against one Book at the same time, each writing `per_thread` entries.
///
/// The graph is shared through an `Arc`, not cloned per writer, so this is genuinely concurrent work against one store
/// and not sixteen stores that add up to a page of results. Each writer takes its own slice of the sequence numbers, so
/// a lost or duplicated record shows up as a wrong total rather than as a coincidence.
pub fn concurrent_writers(
    graph: &Arc<MemoryGraph>,
    book: &str,
    threads: usize,
    per_thread: usize,
) -> Writers {
    let mut handles = Vec::with_capacity(threads);
    for thread in 0..threads {
        let graph = Arc::clone(graph);
        let book = book.to_string();
        handles.push(std::thread::spawn(move || {
            let scope = BookScope::new("scale-user", &book);
            for index in 0..per_thread {
                let sequence = thread * per_thread + index;
                // one unit per entry, so the exact total is arithmetic a reader can check by eye
                let amount = Money::parse("1").expect("one");
                expense(&*graph, &scope, &amount, sequence).expect("a concurrent write");
            }
        }));
    }
    for handle in handles {
        handle
            .join()
            .expect("a writer panicked, which is the failure this suite is looking for");
    }
    let scope = BookScope::new("scale-user", book);
    let written = Records::new(&**graph, EXPENSE)
        .list(&scope, threads * per_thread + 1)
        .expect("the entries just written")
        .len();
    Writers {
        threads,
        per_thread,
        written,
    }
}
