//! The opening trial balance: what a business already had and already owed on the day its books start.
//!
//! A business rarely starts from nothing. It arrives at a new set of books holding stock, owed money, a bank balance, a loan
//! and capital that has been building for years. Those figures are the first thing its books have to know, because every
//! month after them is measured from them: a profit means more than what was there on the first day.
//!
//! What this keeps, and why:
//!
//! * the opening set must balance, and it is refused when it does not, naming the difference. A trial balance that does not
//!   balance is not a rounding problem to be spread: it means a figure is missing, and the books would carry the error for
//!   as long as they exist;
//! * it is posted once. There is one first day, so a second opening set is refused by name rather than quietly moving every
//!   later month's starting point;
//! * a correction to it is a reversing entry like any other, never an edit: the books record what was believed, and the
//!   reversal stands beside it;
//! * it moves no money and charges no cost. An opening balance is not this year's income or this year's expense, so it never
//!   touches a profit and loss account: it states what is already owned and already owed;
//! * the figures are the Book's own money, because the chart they land in is kept in it. A balance held in another money is
//!   a foreign balance and needs the rate it was carried at, which is a separate question and not one to guess at here;
//! * the accounts are the ones the Book already keeps, named by number. An opening balance cannot bring an account into
//!   being: a chart is what a business decided to keep, and inventing accounts here would leave a chart nobody chose.
//!
//! Nothing here moves money: Vimbai records what a business already holds, in the accounts it already has.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money_or_nothing, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, OPENING_BALANCE};

use crate::ledger_book;
use crate::subsidiary_journals::post_lines;

/// The reference an opening set carries. One set, so one reference: a second is refused by the ledger.
const OPENING_REFERENCE: &str = "OPENING";

/// One line of the opening trial balance, as the caller stated it.
struct Stated {
    account: String,
    debit: Money,
    credit: Money,
}

/// The lines the caller stated, in the order they gave them, each moving one side or the other.
fn stated_lines(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Vec<Stated>, StoreError> {
    let rows = payload
        .get("lines")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            refused(
                "An opening trial balance is a list of lines: which account, and what it holds.",
            )
        })?;
    if rows.is_empty() {
        return Err(refused(
            "An opening trial balance with no lines states nothing. Say what the business already holds and owes.",
        ));
    }
    let accounts = ledger_book::accounts(graph, scope)?;
    let mut seen: Vec<String> = Vec::new();
    let mut lines = Vec::with_capacity(rows.len());
    for row in rows {
        let number = text(row, "account_number");
        if number.trim().is_empty() {
            return Err(refused(
                "Every opening line names the account it belongs to, by the number the chart gives it.",
            ));
        }
        let number = number.trim().to_string();
        if !accounts
            .iter()
            .any(|account| field(account, "account_number") == number)
        {
            return Err(refused(&format!(
                "The chart has no account {number}. An opening balance lands in an account the business already keeps; \
                 it cannot bring one into being."
            )));
        }
        if seen.contains(&number) {
            return Err(refused(&format!(
                "Account {number} appears twice in the opening set. State one figure for it, net of what it holds both ways."
            )));
        }
        seen.push(number.clone());
        let debit = money_or_nothing(row, "debit")?;
        let credit = money_or_nothing(row, "credit")?;
        if debit.is_negative() || credit.is_negative() {
            return Err(refused(&format!(
                "Account {number} states a figure less than nothing. Say the side it sits on with a figure that is not negative."
            )));
        }
        if debit.is_zero() && credit.is_zero() {
            return Err(refused(&format!(
                "Account {number} moves nothing. Leave it out of the opening set rather than stating a line of nothing."
            )));
        }
        if !debit.is_zero() && !credit.is_zero() {
            return Err(refused(&format!(
                "Account {number} states both a debit and a credit. State one figure for it, net of what it holds both ways."
            )));
        }
        lines.push(Stated {
            account: number,
            debit,
            credit,
        });
    }
    Ok(lines)
}

/// `POST /opening-balances`: state what the business already had and already owed on its first day.
///
/// The set must balance; the entry is dated the day the caller gives, which is the first day of the books; and it is posted
/// once. What it does not do is move money or touch a profit and loss account.
pub fn post_opening_balances(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entry_date = text(payload, "entry_date").trim().to_string();
    if entry_date.is_empty() {
        return Err(refused(
            "Say the first day of these books, as 2026-01-01. An opening balance belongs to a day, and every month after it \
             is measured from there.",
        ));
    }
    let lines = stated_lines(graph, scope, payload)?;

    // what the two sides come to, worked out here so the refusal can name the difference rather than say "unbalanced"
    let mut debits = Money::zero();
    let mut credits = Money::zero();
    for line in &lines {
        debits = debits.plus(&line.debit);
        credits = credits.plus(&line.credit);
    }
    if debits != credits {
        let difference = if debits > credits {
            debits.minus(&credits)
        } else {
            credits.minus(&debits)
        };
        return Err(refused(&format!(
            "The opening set does not balance: it states {debits} on the debit side and {credits} on the credit side, \
             {} apart. Find the figure that is missing rather than spreading the difference.",
            difference
        )));
    }

    let what = text(payload, "notes");
    let description = if what.trim().is_empty() {
        format!("Opening balances as at {entry_date}")
    } else {
        format!("Opening balances as at {entry_date}: {what}")
    };
    let dated = json!({"entry_date": entry_date, "created_at": text(payload, "created_at")});
    let stated: Vec<(String, Money, Money, String)> = lines
        .iter()
        .map(|line| {
            (
                line.account.clone(),
                line.debit.clone(),
                line.credit.clone(),
                format!("Opening balance for account {}", line.account),
            )
        })
        .collect();
    // the ledger refuses a reference it has already posted, which is what stops a second first day
    let entry_id = post_lines(
        graph,
        scope,
        &dated,
        description,
        OPENING_REFERENCE,
        "OpeningBalances",
        &stated,
    )?;
    let accounts: Vec<Value> = lines
        .iter()
        .map(|line| {
            json!({
                "account_number": line.account,
                "debit": wire(&line.debit),
                "credit": wire(&line.credit),
            })
        })
        .collect();
    let set = json!({
        "entry_date": entry_date,
        "reference_number": OPENING_REFERENCE,
        "debits": wire(&debits),
        "credits": wire(&credits),
        "accounts": accounts,
        "journal_entry_id": entry_id,
        "notes": what,
        "created_at": text(payload, "created_at"),
    });
    remember(graph, scope, OPENING_BALANCE, set)
}

/// `GET /opening-balances`: the opening set this Book carries, and nothing when it has never stated one.
///
/// One set, so this is a single answer rather than a list: a Book either has a first day or it does not.
pub fn opening_balances<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let sets = Records::new(graph, OPENING_BALANCE).list(scope, 1)?;
    match sets.into_iter().next() {
        Some(set) => Ok(set),
        None => Ok(json!({
            "entry_date": "",
            "reference_number": "",
            "debits": wire(&Money::zero()),
            "credits": wire(&Money::zero()),
            "accounts": [],
            "journal_entry_id": "",
            "notes": "",
            "created_at": "",
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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

    /// A business arriving with stock, a bank balance, a loan and capital: what it already had and already owed.
    fn arrived() -> Value {
        json!({
            "entry_date": "2026-01-01",
            "lines": [
                {"account_number": "1010", "debit": "12000.00"},
                {"account_number": "1200", "debit": "8000.00"},
                {"account_number": "2000", "credit": "5000.00"},
                {"account_number": "3000", "credit": "15000.00"}
            ],
            "notes": "Brought forward from the old books"
        })
    }

    #[test]
    fn what_a_business_already_had_and_owed_opens_its_books() {
        let (graph, scope) = business();
        let set = post_opening_balances(&graph, &scope, &arrived()).expect("an opening set");

        assert_eq!(field(&set, "entry_date"), "2026-01-01");
        assert_eq!(field(&set, "debits"), "20000.000000");
        assert_eq!(field(&set, "credits"), "20000.000000");

        let entry = lines(&graph, &scope, field(&set, "journal_entry_id"));
        assert_eq!(entry.len(), 4);
        // each figure lands on the side it was stated on, and the day is the first day of the books
        assert_eq!(
            money(line_on(&entry, "1010"), "debit").expect("a debit"),
            amount("12000")
        );
        assert_eq!(
            money(line_on(&entry, "1200"), "debit").expect("a debit"),
            amount("8000")
        );
        assert_eq!(
            money(line_on(&entry, "2000"), "credit").expect("a credit"),
            amount("5000")
        );
        assert_eq!(
            money(line_on(&entry, "3000"), "credit").expect("a credit"),
            amount("15000")
        );
    }

    #[test]
    fn an_opening_set_that_does_not_balance_is_refused_and_names_the_difference() {
        let (graph, scope) = business();
        let mut set = arrived();
        set["lines"][3]["credit"] = json!("14000.00");

        let refused = post_opening_balances(&graph, &scope, &set).expect_err("unbalanced");
        let said = refused.to_string();
        // a trial balance that does not balance is a missing figure, not a rounding problem to spread
        assert!(said.contains("does not balance"), "{said}");
        assert!(said.contains("1000"), "{said}");
        // and nothing was written
        assert!(Records::new(&graph, OPENING_BALANCE)
            .list(&scope, 10)
            .expect("the records")
            .is_empty());
    }

    #[test]
    fn there_is_one_first_day_so_a_second_set_is_refused() {
        let (graph, scope) = business();
        post_opening_balances(&graph, &scope, &arrived()).expect("an opening set");

        let again = post_opening_balances(&graph, &scope, &arrived()).expect_err("a second set");
        assert!(
            again.to_string().to_lowercase().contains("already"),
            "the refusal says the reference is taken: {again}"
        );
        // and the Book still carries one set, not two
        assert_eq!(
            Records::new(&graph, OPENING_BALANCE)
                .list(&scope, 10)
                .expect("the records")
                .len(),
            1
        );
    }

    #[test]
    fn an_account_the_chart_does_not_keep_is_refused_by_name() {
        let (graph, scope) = business();
        let set = json!({
            "entry_date": "2026-01-01",
            "lines": [
                {"account_number": "1010", "debit": "500.00"},
                {"account_number": "9999", "credit": "500.00"}
            ]
        });

        let refused = post_opening_balances(&graph, &scope, &set).expect_err("no such account");
        // an opening balance cannot bring an account into being: a chart is what the business decided to keep
        assert!(refused.to_string().contains("9999"), "{refused}");
        assert!(refused.to_string().contains("chart"), "{refused}");
    }

    #[test]
    fn a_line_that_states_both_sides_or_neither_is_refused() {
        let (graph, scope) = business();
        let both = json!({
            "entry_date": "2026-01-01",
            "lines": [{"account_number": "1010", "debit": "500.00", "credit": "500.00"}]
        });
        let said = post_opening_balances(&graph, &scope, &both)
            .expect_err("both sides")
            .to_string();
        assert!(said.contains("both a debit and a credit"), "{said}");

        let neither = json!({
            "entry_date": "2026-01-01",
            "lines": [{"account_number": "1010", "debit": "0", "credit": "0"}]
        });
        let said = post_opening_balances(&graph, &scope, &neither)
            .expect_err("neither side")
            .to_string();
        assert!(said.contains("moves nothing"), "{said}");
    }

    #[test]
    fn an_account_stated_twice_is_refused_because_one_figure_means_one_net() {
        let (graph, scope) = business();
        let set = json!({
            "entry_date": "2026-01-01",
            "lines": [
                {"account_number": "1010", "debit": "500.00"},
                {"account_number": "1010", "credit": "200.00"}
            ]
        });

        let refused = post_opening_balances(&graph, &scope, &set).expect_err("twice");
        assert!(refused.to_string().contains("appears twice"), "{refused}");
    }

    #[test]
    fn an_opening_set_with_no_lines_or_no_day_states_nothing() {
        let (graph, scope) = business();
        let no_lines = json!({"entry_date": "2026-01-01", "lines": []});
        assert!(post_opening_balances(&graph, &scope, &no_lines)
            .expect_err("no lines")
            .to_string()
            .contains("states nothing"));

        let no_day = json!({"lines": [{"account_number": "1010", "debit": "5.00"}]});
        assert!(post_opening_balances(&graph, &scope, &no_day)
            .expect_err("no day")
            .to_string()
            .contains("first day"));
    }

    #[test]
    fn a_books_first_day_is_read_back_and_an_empty_one_says_so_plainly() {
        let (graph, scope) = business();
        let nothing = opening_balances(&graph, &scope).expect("an answer");
        assert_eq!(field(&nothing, "entry_date"), "");
        assert_eq!(field(&nothing, "debits"), "0.000000");

        post_opening_balances(&graph, &scope, &arrived()).expect("an opening set");
        let set = opening_balances(&graph, &scope).expect("an answer");
        assert_eq!(field(&set, "entry_date"), "2026-01-01");
        assert_eq!(set["accounts"].as_array().expect("accounts").len(), 4);
    }
}
