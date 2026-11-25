//! A debt the business has given up on.
//!
//! A customer who will not pay is ordinary trading, and the books have to say so plainly. Until it is written off, the
//! money they owe sits in receivables as though it were coming, so the balance sheet claims an asset the business does not
//! have and the profit already taken on the sale stands on a sale that will not be paid for.
//!
//! What this keeps, and why:
//!
//! * the write-off takes the debt out of receivables and puts the cost where it belongs, in the month the business gave up
//!   on it: Dr Bad Debts, Cr Accounts Receivable. The sale itself is not touched, because the sale happened;
//! * it is a reversing entry away from being wrong, never an edit: the books record what was believed, and giving up on a
//!   debt is a decision dated the day it was taken;
//! * a month that has been closed is not written into, because its profit has been reported and signed;
//! * the same debtor is not written off twice on the same day, which is what a second press of the button would do: the
//!   reference names the debtor and the day, and the ledger refuses a reference it has already posted;
//! * the accounts are found by what the Book's chart calls them and by their type, never by a number, so a Book that numbers
//!   its own chart still works, and a caller may name the accounts when its chart words them differently;
//! * nothing here moves money. A write-off is a decision that money will not arrive; it never pays anybody or collects
//!   anything.
//!
//! A debt that might not be paid but has not been given up on is a different question - a provision, which is an estimate
//! adjusted as the year goes on - and is not posted here.

use serde_json::{json, Value};
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, BAD_DEBT};

use crate::periods;
use crate::subsidiary_journals::{account_named, must_be_open, positive, post};

/// The day a write-off is dated, and the month it belongs to.
///
/// A write-off belongs to the day the business decided the money was not coming, so the day is what is asked for rather
/// than a month: the decision has a date and the month follows from it.
fn day_of(payload: &Value) -> Result<String, StoreError> {
    let stated = text(payload, "entry_date").trim().to_string();
    if stated.is_empty() {
        return Err(refused(
            "Say the day the business gave up on this debt, as 2026-11-20. The month follows from it.",
        ));
    }
    // the month a day belongs to is read the same way everywhere, so a day that does not read is refused here
    if periods::month_of(&stated).is_none() {
        return Err(refused(&format!(
            "{stated} does not read as a day. Say it as 2026-11-20."
        )));
    }
    Ok(stated)
}

/// `POST /bad-debts`: give up on a debt, taking it out of receivables and into the cost of the month.
///
/// The sale is left alone: it happened. What changes is that the money is no longer expected.
pub fn write_off(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entry_date = day_of(payload)?;
    must_be_open(graph, scope, &entry_date)?;
    let debtor = text(payload, "debtor").trim().to_string();
    if debtor.is_empty() {
        return Err(refused(
            "Say who owed the money: a name or the reference the business knows them by.",
        ));
    }
    let amount = positive(payload, "amount", "A debt written off")?;
    let cost = account_named(
        graph,
        scope,
        payload,
        "expense_account",
        "bad_debts",
        "expense",
    )?;
    let owed = account_named(
        graph,
        scope,
        payload,
        "receivable_account",
        "receivable",
        "asset",
    )?;
    if cost == owed {
        return Err(refused(
            "The cost and the money owed cannot be the same account: one is what was lost and the other is where it sat.",
        ));
    }
    let why = text(payload, "reason");
    let note = if why.trim().is_empty() {
        format!("Written off: {debtor}")
    } else {
        format!("Written off: {debtor} ({why})")
    };
    // one write-off per debtor per day: the ledger refuses a reference it has already posted, so a second press cannot
    // take the same debt out of the books twice
    let reference = format!("BAD-{entry_date}-{debtor}");
    let dated = json!({"entry_date": entry_date, "created_at": text(payload, "created_at")});
    let entry_id = post(
        graph,
        scope,
        &dated,
        format!("Bad debt: {debtor}"),
        &reference,
        "BadDebts",
        &cost,
        &owed,
        &amount,
        &note,
    )?;
    remember(
        graph,
        scope,
        BAD_DEBT,
        json!({
            "entry_date": entry_date,
            "debtor": debtor,
            "amount": wire(&amount),
            "expense_account": cost,
            "receivable_account": owed,
            "reason": why,
            "journal_entry_id": entry_id,
            "notes": text(payload, "notes"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /bad-debts`: the debts this Book has given up on, newest first, narrowed by debtor or month when asked.
pub fn list_written_off<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let debtor = text(query, "debtor");
    let period = text(query, "period");
    let mut rows: Vec<Value> = Records::new(graph, BAD_DEBT)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| {
            let debtor_said = debtor.trim().is_empty()
                || field(row, "debtor")
                    .to_lowercase()
                    .contains(&debtor.trim().to_lowercase());
            let period_said = period.trim().is_empty()
                || periods::month_of(field(row, "entry_date")).as_deref() == Some(period.trim());
            debtor_said && period_said
        })
        .collect();
    rows.sort_by(|a, b| {
        field(b, "entry_date")
            .cmp(field(a, "entry_date"))
            .then_with(|| field(a, "debtor").cmp(field(b, "debtor")))
    });
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_common::Money;
    use vimbai_store::fake::MemoryGraph;
    use vimbai_store::read::money;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn business() -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = scope();
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        (graph, scope)
    }

    fn amount(text: &str) -> Money {
        Money::parse(text).expect("an amount")
    }

    fn lines(graph: &MemoryGraph, scope: &BookScope, entry_id: &str) -> Vec<Value> {
        crate::ledger_book::get_entry(graph, scope, entry_id).expect("the entry")["lines"]
            .as_array()
            .cloned()
            .expect("lines")
    }

    fn line_on<'a>(lines: &'a [Value], account: &str) -> &'a Value {
        lines
            .iter()
            .find(|one| field(one, "account_number") == account)
            .unwrap_or_else(|| panic!("no line on {account}"))
    }

    fn owed() -> Value {
        json!({"entry_date": "2026-11-20", "debtor": "Tendai Moyo", "amount": "450.00",
               "reason": "Moved away, cannot be traced"})
    }

    #[test]
    fn a_debt_given_up_on_leaves_receivables_and_is_charged_to_the_month() {
        let (graph, scope) = business();
        let written = write_off(&graph, &scope, &owed()).expect("a write-off");

        assert_eq!(field(&written, "debtor"), "Tendai Moyo");
        assert_eq!(field(&written, "amount"), "450.000000");
        assert_eq!(field(&written, "entry_date"), "2026-11-20");

        let entry = lines(&graph, &scope, field(&written, "journal_entry_id"));
        // the cost is charged and the money owed comes off the books: the sale itself is untouched
        assert_eq!(
            money(line_on(&entry, "6350"), "debit").expect("a debit"),
            amount("450")
        );
        assert_eq!(
            money(line_on(&entry, "1100"), "credit").expect("a credit"),
            amount("450")
        );
        // and the entry is dated the day the business decided, not the day somebody pressed the button
        let posted =
            crate::ledger_book::get_entry(&graph, &scope, field(&written, "journal_entry_id"))
                .expect("the entry");
        assert_eq!(field(&posted, "entry_date"), "2026-11-20");
        assert_eq!(field(&posted, "source_module"), "BadDebts");
    }

    #[test]
    fn the_same_debt_cannot_be_written_off_twice_on_the_same_day() {
        let (graph, scope) = business();
        write_off(&graph, &scope, &owed()).expect("a write-off");

        let again = write_off(&graph, &scope, &owed()).expect_err("a second write-off");
        // the ledger refuses the reference it has already posted, so a second press cannot take the debt out twice
        assert!(
            again.to_string().to_lowercase().contains("already"),
            "{again}"
        );
        assert_eq!(
            Records::new(&graph, BAD_DEBT)
                .list(&scope, 10)
                .expect("the records")
                .len(),
            1
        );
    }

    #[test]
    fn a_debtor_is_named_and_a_write_off_states_something() {
        let (graph, scope) = business();
        let nameless = json!({"entry_date": "2026-11-20", "amount": "50.00"});
        let said = write_off(&graph, &scope, &nameless)
            .expect_err("no debtor")
            .to_string();
        assert!(said.contains("who owed the money"), "{said}");

        let nothing = json!({"entry_date": "2026-11-20", "debtor": "A Customer", "amount": "0"});
        let said = write_off(&graph, &scope, &nothing)
            .expect_err("nothing")
            .to_string();
        assert!(said.contains("more than nothing"), "{said}");
    }

    #[test]
    fn a_day_that_does_not_read_as_a_day_is_refused() {
        let (graph, scope) = business();
        let vague = json!({"entry_date": "November", "debtor": "A Customer", "amount": "10.00"});
        let said = write_off(&graph, &scope, &vague)
            .expect_err("not a day")
            .to_string();
        assert!(said.contains("does not read as a day"), "{said}");

        let none = json!({"debtor": "A Customer", "amount": "10.00"});
        let said = write_off(&graph, &scope, &none)
            .expect_err("no day")
            .to_string();
        assert!(said.contains("gave up on this debt"), "{said}");
    }

    #[test]
    fn a_closed_month_is_not_written_into() {
        let (graph, scope) = business();
        crate::periods::close_period(
            &graph,
            &scope,
            &json!({"period": "2026-11", "closes_on": "2026-11-30"}),
        )
        .expect("closed");

        let said = write_off(&graph, &scope, &owed())
            .expect_err("closed month")
            .to_string();
        assert!(said.to_lowercase().contains("closed"), "{said}");
        // and nothing was written
        assert!(Records::new(&graph, BAD_DEBT)
            .list(&scope, 10)
            .expect("the records")
            .is_empty());
    }

    #[test]
    fn the_debts_given_up_on_are_listed_newest_first_and_narrow_by_debtor_and_month() {
        let (graph, scope) = business();
        write_off(&graph, &scope, &owed()).expect("a write-off");
        write_off(
            &graph,
            &scope,
            &json!({"entry_date": "2026-12-02", "debtor": "Chipo Ncube", "amount": "120.00"}),
        )
        .expect("another write-off");

        let all = list_written_off(&graph, &scope, &json!({})).expect("a list");
        let rows = all.as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        // newest first: the December one is the later decision
        assert_eq!(field(&rows[0], "debtor"), "Chipo Ncube");

        let one = list_written_off(&graph, &scope, &json!({"debtor": "tendai"})).expect("a list");
        assert_eq!(one.as_array().expect("rows").len(), 1);
        assert_eq!(field(&one[0], "debtor"), "Tendai Moyo");

        let november =
            list_written_off(&graph, &scope, &json!({"period": "2026-11"})).expect("a list");
        assert_eq!(november.as_array().expect("rows").len(), 1);
        assert_eq!(field(&november[0], "debtor"), "Tendai Moyo");
    }
}
