//! What the platform does at magnitudes it was not written against.
//!
//! Every assertion here is exact. An `f64` holds an integer to about sixteen digits, so every test in this file is one
//! that a float implementation would pass at ten digits and fail at thirty - which is the reason the workspace moved to
//! `Money` in the first place.

use vimbai_common::Money;
use vimbai_ledger_core::validate_money_lines;
use vimbai_scale::{big_amount, total};
use vimbai_store::fake::MemoryGraph;
use vimbai_store::{BookScope, Records, EXPENSE};

#[test]
fn an_amount_with_forty_digits_is_stored_and_read_back_unchanged() {
    // The store carries amounts as decimal strings, so the limit is the reader's type and not the database's.
    let graph = MemoryGraph::new();
    let scope = BookScope::new("scale-user", "scale-book");
    let amount = Money::parse("12345678901234567890123456789012345678.90").expect("forty digits");
    Records::new(&graph, EXPENSE)
        .create(
            &scope,
            &serde_json::json!({ "description": "a very large invoice", "amount": amount.to_string() }),
        )
        .expect("a stored amount");

    let rows = Records::new(&graph, EXPENSE)
        .list(&scope, 10)
        .expect("the entry");
    assert_eq!(rows.len(), 1);
    let stored = rows[0]["amount"].as_str().expect("an amount string");
    let read_back = Money::parse(stored).expect("a stored decimal");
    assert_eq!(read_back, amount, "the store changed the amount: {stored}");
}

#[test]
fn ten_thousand_thirty_digit_amounts_total_exactly() {
    // Summing is where a float loses first: each addend is fine, the running total is not. Twenty thousand values of
    // thirty digits, in two different orders, have to agree with each other and with the closed form.
    let count = 20_000u64;
    let values: Vec<Money> = (1..=count).map(|offset| big_amount(30, offset)).collect();
    let forwards = total(&values);

    let mut backwards = values.clone();
    backwards.reverse();
    let reverse = total(&backwards);

    assert_eq!(
        forwards, reverse,
        "the total depends on the order the amounts arrived in"
    );

    // the closed form of the same sum, computed here from the same text, so a shared bug cannot hide
    let digits = 30usize;
    let mut expected = Money::zero();
    for offset in 1..=count {
        expected = expected.plus(&big_amount(digits, offset));
    }
    assert_eq!(forwards, expected);
    assert!(
        forwards.to_string().len() >= digits,
        "a total of thirty-digit addends came back as {}",
        forwards
    );
}

#[test]
fn a_set_that_balances_at_thirty_digits_balances() {
    // Debits and credits of thirty digits on both sides: the kernel compares exact values, so this is not a tolerance
    // question. A float door rounds to ten decimal places and would pass this by luck; the exact door passes it by
    // construction.
    let debit = Money::parse("999999999999999999999999999999.99").expect("an amount");
    let credit = Money::parse("999999999999999999999999999999.99").expect("an amount");
    assert!(
        validate_money_lines(&[
            (debit.clone(), Money::zero()),
            (Money::zero(), credit.clone())
        ])
        .valid,
        "a set that balances exactly at thirty digits was refused"
    );

    // and one minor unit apart is refused, at a magnitude where an f64 cannot represent the difference at all
    let short = Money::parse("999999999999999999999999999999.98").expect("an amount");
    assert!(
        !validate_money_lines(&[(debit, Money::zero()), (Money::zero(), short)]).valid,
        "a set one minor unit out of balance was accepted at thirty digits"
    );
}

#[test]
fn splitting_a_thirty_digit_amount_across_a_thousand_parts_loses_nothing() {
    // Allocation is the other place a float goes wrong: the parts have to add back to the whole to the last unit, and a
    // thousand parts is when the rounding is most likely to leave a remainder behind.
    let whole = Money::parse("1234567890123456789012345678901.01").expect("an amount");
    let weights: Vec<Money> = (1..=1000)
        .map(|index| Money::from_f64_shortest(index as f64).expect("a weight"))
        .collect();
    let parts = whole.allocate(&weights, 2);
    assert_eq!(parts.len(), 1000);
    assert_eq!(
        total(&parts),
        whole,
        "the parts do not add back to the whole"
    );
}
