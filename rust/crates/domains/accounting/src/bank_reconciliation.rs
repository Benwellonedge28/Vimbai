//! Matching a bank statement against the cash book.
//!
//! A reconciliation is where a person used to retype what the books already held: the statement arrives with its
//! lines, and the record it produced asked for the differences to be typed in beside a statement balance. This reads
//! both sides and answers three questions - what matches, what the statement carries and the books do not, and what
//! the books hold and the statement does not - and then proves that the difference between the two balances is
//! exactly those unmatched lines.
//!
//! The rules, as this reads them:
//!
//! * the account is named by the code the Book keeps it under, and a code the Book does not keep is refused,
//! * only posted cash entries are matched: a pending entry is not yet a fact the bank could have seen,
//! * a statement line names a date written `YYYY-MM-DD`, an amount greater than nothing, and a side - `in` for money
//!   arriving, `out` for money leaving. A negative amount is refused rather than guessed at, because a minus sign on
//!   a statement means the opposite of what it means in the books,
//! * matching is one to one: an entry answers at most one line and a line at most one entry, on the exact amount and
//!   the same side, preferring a line and an entry that carry the same reference, then the same date,
//! * the difference between the statement's closing balance and the book's balance is explained by the unmatched
//!   lines. What is left over is named as unexplained rather than rounded away, and it means one of two things: the
//!   two sides do not start from the same opening figure, or a line was read as the wrong side.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, stored_amount, wire};
use vimbai_store::read::{field, flag, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, CASH_ENTRY};

use crate::cashbook::account_by_code;

/// One line of the statement the bank sent.
struct StatementLine {
    number: usize,
    date: String,
    reference: String,
    description: String,
    /// Positive for money arriving, negative for money leaving.
    signed: Money,
    /// `in` or `out`, as the statement reads.
    side: &'static str,
}

/// One posted cash entry, as the matching reads it.
struct BookLine {
    entry_id: String,
    date: String,
    reference: String,
    description: String,
    /// Positive for a debit to the account, negative for a credit.
    signed: Money,
}

fn absolute(value: &Money) -> Money {
    if value.is_negative() {
        value.negate()
    } else {
        value.clone()
    }
}

fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

/// The statement's lines, refused one at a time with the line that was wrong.
fn statement_lines(payload: &Value) -> Result<Vec<StatementLine>, StoreError> {
    let rows = match payload.get("lines") {
        Some(Value::Array(rows)) => rows.clone(),
        _ => {
            return Err(StoreError::Invalid(
                "a reconciliation carries the statement's lines under `lines`; none were given"
                    .into(),
            ))
        }
    };
    let mut lines = Vec::with_capacity(rows.len());
    for (index, raw) in rows.iter().enumerate() {
        let number = index + 1;
        let date = text(raw, "date");
        if !is_date(&date) {
            return Err(StoreError::Invalid(format!(
                "statement line {number} needs a date written as YYYY-MM-DD, and it carries `{date}`"
            )));
        }
        let value = amount(raw, "amount")?;
        if value.is_negative() {
            return Err(StoreError::Invalid(format!(
                "statement line {number} carries a negative amount; say which way the money went with `side` instead"
            )));
        }
        if value == Money::zero() {
            return Err(StoreError::Invalid(format!(
                "statement line {number} carries nothing, so there is no movement to match"
            )));
        }
        let side = match raw.get("side").and_then(Value::as_str) {
            Some("in") => "in",
            Some("out") => "out",
            Some(other) => {
                return Err(StoreError::Invalid(format!(
                    "statement line {number} says its side is `{other}`; a side is `in` for money arriving or `out` \
                     for money leaving"
                )))
            }
            None => {
                return Err(StoreError::Invalid(format!(
                    "statement line {number} does not say which way the money went; give `side` as `in` or `out`"
                )))
            }
        };
        let signed = if side == "in" { value } else { value.negate() };
        lines.push(StatementLine {
            number,
            date,
            reference: text(raw, "reference").trim().to_string(),
            description: text(raw, "description"),
            signed,
            side,
        });
    }
    Ok(lines)
}

/// The posted entries the Book holds against the account.
fn book_lines<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_code: &str,
) -> Result<Vec<BookLine>, StoreError> {
    let mut lines = Vec::new();
    for row in Records::new(graph, CASH_ENTRY).list(scope, 10000)? {
        if field(&row, "bank_account") != account_code || field(&row, "status") != "posted" {
            continue;
        }
        let base = stored_amount(&row, "base_amount")?;
        let signed = if flag(&row, "is_debit") {
            base
        } else {
            base.negate()
        };
        lines.push(BookLine {
            entry_id: row
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            date: field(&row, "entry_date").to_string(),
            reference: field(&row, "reference").trim().to_string(),
            description: field(&row, "description").to_string(),
            signed,
        });
    }
    lines.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    Ok(lines)
}

/// How well a statement line and a book line look like the same movement: 0 is the same reference on the same day,
/// 3 is nothing beyond the amount and the side.
fn rank(line: &StatementLine, entry: &BookLine) -> u8 {
    let same_reference = !line.reference.is_empty()
        && line.reference.eq_ignore_ascii_case(&entry.reference)
        && !entry.reference.is_empty();
    let same_date = line.date == entry.date;
    match (same_reference, same_date) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    }
}

/// `POST /bank-reconciliation/match`: match a statement against the cash book and prove the difference between them.
///
/// The older `/bank-reconciliation` door records a working paper a preparer and a reviewer sign; this one does the
/// matching that paper is about, from the entries themselves rather than from differences typed in beside it.
pub fn reconcile<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let account_code = text(payload, "bank_account");
    if account_code.trim().is_empty() {
        return Err(StoreError::Invalid(
            "a reconciliation names the cash account it is for, by its account code".into(),
        ));
    }
    let account = account_by_code(graph, scope, &account_code)?.ok_or_else(|| {
        StoreError::Invalid(format!(
            "the Book keeps no cash account under the code `{account_code}`, so there is nothing to reconcile it \
             against"
        ))
    })?;
    let opening = stored_amount(&account, "opening_balance")?;

    let lines = statement_lines(payload)?;
    let entries = book_lines(graph, scope, &account_code)?;

    // one to one, in the order the statement lists its lines, and never reusing an entry
    let mut taken = vec![false; entries.len()];
    let mut matched: Vec<Value> = Vec::new();
    let mut statement_only: Vec<Value> = Vec::new();
    let mut matched_net = Money::zero();
    let mut statement_only_net = Money::zero();

    for line in &lines {
        let mut best: Option<(u8, usize)> = None;
        for (index, entry) in entries.iter().enumerate() {
            if taken[index] || entry.signed != line.signed {
                continue;
            }
            let score = rank(line, entry);
            if best.map(|(lowest, _)| score < lowest).unwrap_or(true) {
                best = Some((score, index));
            }
        }
        match best {
            Some((_, index)) => {
                taken[index] = true;
                let entry = &entries[index];
                matched_net = matched_net.plus(&line.signed);
                matched.push(json!({
                    "statement_line": line.number,
                    "statement_date": line.date,
                    "statement_reference": line.reference,
                    "statement_description": line.description,
                    "side": line.side,
                    "amount": wire(&absolute(&line.signed)),
                    "entry_id": entry.entry_id,
                    "entry_date": entry.date,
                    "entry_reference": entry.reference,
                    "entry_description": entry.description,
                }));
            }
            None => {
                statement_only_net = statement_only_net.plus(&line.signed);
                statement_only.push(json!({
                    "statement_line": line.number,
                    "date": line.date,
                    "reference": line.reference,
                    "description": line.description,
                    "side": line.side,
                    "amount": wire(&absolute(&line.signed)),
                    "signed": wire(&line.signed),
                }));
            }
        }
    }

    let mut book_only: Vec<Value> = Vec::new();
    let mut book_only_net = Money::zero();
    let mut book_balance = opening.clone();
    for (index, entry) in entries.iter().enumerate() {
        book_balance = book_balance.plus(&entry.signed);
        if taken[index] {
            continue;
        }
        book_only_net = book_only_net.plus(&entry.signed);
        book_only.push(json!({
            "entry_id": entry.entry_id,
            "entry_date": entry.date,
            "reference": entry.reference,
            "description": entry.description,
            "signed": wire(&entry.signed),
            "amount": wire(&absolute(&entry.signed)),
            "side": if entry.signed.is_negative() { "out" } else { "in" },
        }));
    }

    let statement_balance = amount(payload, "statement_balance")?;
    let difference = statement_balance.minus(&book_balance);
    // the statement's own figure, less what the books say, is exactly the net of the lines each side holds alone
    let explained = statement_only_net.minus(&book_only_net);
    let unexplained = difference.minus(&explained);
    let agrees = unexplained == Money::zero();

    let summary = if agrees {
        format!(
            "every figure on the statement is either in the books or named here: {} matched, {} on the statement \
             alone and {} in the books alone",
            matched.len(),
            statement_only.len(),
            book_only.len()
        )
    } else {
        format!(
            "the two sides differ by {} beyond the lines named here, so either they do not start from the same \
             opening figure or a line was read as the wrong side",
            wire(&unexplained)
        )
    };

    Ok(json!({
        "bank_account": account_code,
        "account_name": field(&account, "account_name"),
        "statement_balance": wire(&statement_balance),
        "book_balance": wire(&book_balance),
        "opening_balance": wire(&opening),
        "difference": wire(&difference),
        "statement_lines": lines.len(),
        "posted_entries": entries.len(),
        "matched": matched,
        "in_statement_only": statement_only,
        "in_books_only": book_only,
        "matched_net": wire(&matched_net),
        "statement_only_net": wire(&statement_only_net),
        "book_only_net": wire(&book_only_net),
        "explained_difference": wire(&explained),
        "unexplained_difference": wire(&unexplained),
        "agrees": agrees,
        "summary": summary,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashbook::{create_account, create_entry};
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A Book with a bank account opened at 1,000.00 and nothing in it yet.
    fn book() -> MemoryGraph {
        let graph = MemoryGraph::new();
        create_account(
            &graph,
            &scope(),
            &json!({"account_code": "1020", "account_name": "Current account", "opening_balance": "1000.00"}),
        )
        .expect("a bank account");
        graph
    }

    fn entry(
        graph: &MemoryGraph,
        date: &str,
        amount: &str,
        debit: bool,
        reference: &str,
        status: &str,
    ) {
        create_entry(
            graph,
            &scope(),
            &json!({"bank_account": "1020", "entry_date": date, "amount": amount, "is_debit": debit,
                    "reference": reference, "description": format!("entry {reference}"), "status": status,
                    "voucher_number": reference}),
        )
        .expect("an entry");
    }

    fn reconcile_with(
        graph: &MemoryGraph,
        lines: Value,
        balance: &str,
    ) -> Result<Value, StoreError> {
        reconcile(
            graph,
            &scope(),
            &json!({"bank_account": "1020", "statement_balance": balance, "lines": lines}),
        )
    }

    #[test]
    fn a_line_the_books_hold_is_matched_to_its_entry() {
        let graph = book();
        entry(&graph, "2026-10-05", "150.00", true, "INV-1", "posted");
        let answer = reconcile_with(
            &graph,
            json!([{"date": "2026-10-05", "amount": "150.00", "side": "in", "reference": "INV-1"}]),
            "1150.00",
        )
        .expect("a reconciliation");
        assert_eq!(answer["matched"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            answer["in_statement_only"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(answer["in_books_only"].as_array().map(Vec::len), Some(0));
        assert_eq!(answer["agrees"], json!(true));
        assert_eq!(answer["book_balance"], json!("1150.000000"));
    }

    #[test]
    fn the_difference_is_exactly_the_lines_each_side_holds_alone() {
        let graph = book();
        // in the books, on the statement: a receipt and a payment
        entry(&graph, "2026-10-05", "500.00", true, "INV-1", "posted");
        entry(&graph, "2026-10-09", "200.00", false, "BILL-1", "posted");
        // in the books, not yet on the statement: a cheque nobody has presented
        entry(&graph, "2026-10-28", "75.00", false, "CHQ-9", "posted");
        // on the statement, not in the books: a bank charge the owner has not recorded
        let answer = reconcile_with(
            &graph,
            json!([
                {"date": "2026-10-05", "amount": "500.00", "side": "in", "reference": "INV-1"},
                {"date": "2026-10-09", "amount": "200.00", "side": "out", "reference": "BILL-1"},
                {"date": "2026-10-31", "amount": "12.50", "side": "out", "description": "monthly charge"}
            ]),
            "1287.50",
        )
        .expect("a reconciliation");
        assert_eq!(answer["book_balance"], json!("1225.000000"));
        assert_eq!(answer["difference"], json!("62.500000"));
        assert_eq!(
            answer["in_statement_only"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(answer["in_books_only"].as_array().map(Vec::len), Some(1));
        assert!(answer["in_books_only"][0]["entry_id"].is_string());
        assert_eq!(answer["explained_difference"], json!("62.500000"));
        assert_eq!(answer["unexplained_difference"], json!("0.000000"));
        assert_eq!(answer["agrees"], json!(true));
    }

    #[test]
    fn a_pending_entry_is_not_matched_because_it_is_not_a_fact_yet() {
        let graph = book();
        entry(&graph, "2026-10-05", "150.00", true, "INV-1", "pending");
        let answer = reconcile_with(
            &graph,
            json!([{"date": "2026-10-05", "amount": "150.00", "side": "in", "reference": "INV-1"}]),
            "1000.00",
        )
        .expect("a reconciliation");
        assert_eq!(answer["posted_entries"], json!(0));
        assert_eq!(answer["matched"].as_array().map(Vec::len), Some(0));
        assert_eq!(
            answer["in_statement_only"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(answer["book_balance"], json!("1000.000000"));
    }

    #[test]
    fn the_same_amount_on_the_other_side_is_not_a_match() {
        let graph = book();
        entry(&graph, "2026-10-05", "150.00", false, "PMT-1", "posted");
        let answer = reconcile_with(
            &graph,
            json!([{"date": "2026-10-05", "amount": "150.00", "side": "in"}]),
            "850.00",
        )
        .expect("a reconciliation");
        assert_eq!(answer["matched"].as_array().map(Vec::len), Some(0));
        assert_eq!(answer["in_statement_only"][0]["side"], json!("in"));
        assert_eq!(answer["in_books_only"][0]["side"], json!("out"));
    }

    #[test]
    fn one_entry_answers_at_most_one_line() {
        let graph = book();
        entry(&graph, "2026-10-05", "150.00", true, "INV-1", "posted");
        let answer = reconcile_with(
            &graph,
            json!([
                {"date": "2026-10-05", "amount": "150.00", "side": "in"},
                {"date": "2026-10-06", "amount": "150.00", "side": "in"}
            ]),
            "1150.00",
        )
        .expect("a reconciliation");
        assert_eq!(answer["matched"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            answer["in_statement_only"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(answer["in_books_only"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn a_reference_on_the_same_day_wins_the_match() {
        let graph = book();
        entry(&graph, "2026-10-05", "150.00", true, "INV-1", "posted");
        entry(&graph, "2026-10-05", "150.00", true, "INV-2", "posted");
        let answer = reconcile_with(
            &graph,
            json!([{"date": "2026-10-05", "amount": "150.00", "side": "in", "reference": "inv-2"}]),
            "1300.00",
        )
        .expect("a reconciliation");
        // the reference decides it, whatever order the entries were written in, and case does not matter
        assert_eq!(answer["matched"][0]["entry_reference"], json!("INV-2"));
        assert_eq!(answer["in_books_only"][0]["reference"], json!("INV-1"));
    }

    #[test]
    fn a_negative_amount_is_refused_rather_than_read_as_the_other_side() {
        let graph = book();
        let refused = reconcile_with(
            &graph,
            json!([{"date": "2026-10-05", "amount": "-150.00", "side": "in"}]),
            "1000.00",
        )
        .expect_err("a negative line");
        assert!(refused.to_string().contains("negative amount"), "{refused}");
    }

    #[test]
    fn a_line_that_does_not_say_which_way_the_money_went_is_refused() {
        let graph = book();
        let refused = reconcile_with(
            &graph,
            json!([{"date": "2026-10-05", "amount": "150.00"}]),
            "1000.00",
        )
        .expect_err("no side");
        assert!(refused.to_string().contains("`in` or `out`"), "{refused}");
        let refused = reconcile_with(
            &graph,
            json!([{"date": "2026-10-05", "amount": "150.00", "side": "sideways"}]),
            "1000.00",
        )
        .expect_err("an unknown side");
        assert!(refused.to_string().contains("`sideways`"), "{refused}");
    }

    #[test]
    fn a_line_with_no_date_says_which_line_it_is() {
        let graph = book();
        let refused = reconcile_with(
            &graph,
            json!([{"date": "5 October", "amount": "150.00", "side": "in"}]),
            "1000.00",
        )
        .expect_err("a bad date");
        assert!(refused.to_string().contains("line 1"), "{refused}");
    }

    #[test]
    fn an_account_the_book_does_not_keep_is_refused() {
        let graph = book();
        let refused = reconcile(
            &graph,
            &scope(),
            &json!({"bank_account": "9999", "statement_balance": "0.00", "lines": []}),
        )
        .expect_err("no such account");
        assert!(
            refused
                .to_string()
                .contains("no cash account under the code `9999`"),
            "{refused}"
        );
    }

    #[test]
    fn a_reconciliation_without_the_statements_lines_is_refused() {
        let graph = book();
        let refused = reconcile(
            &graph,
            &scope(),
            &json!({"bank_account": "1020", "statement_balance": "0.00"}),
        )
        .expect_err("no lines");
        assert!(refused.to_string().contains("under `lines`"), "{refused}");
    }

    #[test]
    fn a_difference_the_lines_do_not_explain_names_the_opening_figure() {
        let graph = book();
        entry(&graph, "2026-10-05", "150.00", true, "INV-1", "posted");
        let answer = reconcile_with(
            &graph,
            json!([{"date": "2026-10-05", "amount": "150.00", "side": "in", "reference": "INV-1"}]),
            "2000.00",
        )
        .expect("a reconciliation");
        assert_eq!(answer["agrees"], json!(false));
        assert_eq!(answer["unexplained_difference"], json!("850.000000"));
        assert!(answer["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("opening figure"));
    }

    #[test]
    fn a_statement_with_no_movement_still_answers_what_the_books_hold_alone() {
        let graph = book();
        entry(&graph, "2026-10-28", "75.00", false, "CHQ-9", "posted");
        let answer = reconcile_with(&graph, json!([]), "1000.00").expect("a reconciliation");
        assert_eq!(answer["matched"].as_array().map(Vec::len), Some(0));
        assert_eq!(answer["in_books_only"].as_array().map(Vec::len), Some(1));
        assert_eq!(answer["agrees"], json!(true));
    }
}
