//! The payroll journal: a month's wages, reaching the ledger.
//!
//! Paying staff is the largest cost most businesses carry, and until now it could not reach the books at all. The wages
//! were paid out of the bank and the cash book recorded that, but nothing recorded what the work cost, what the staff were
//! still owed on the day the month closed, or what the business was holding for the revenue authority, the pension fund and
//! everybody else a payslip withholds for. So profit was overstated by a month's wages and the balance sheet was short the
//! liabilities that go with them.
//!
//! What this journal keeps, and why:
//!
//! * **The run is stated, not computed.** A payroll run has already been worked out - by a payroll officer, by the
//!   calculator service, by a schedule - before anybody asks the books to carry it. The figures arrive as stated amounts, and
//!   the only arithmetic here is what must be true of them: net is gross less what was withheld, and the total cost is gross
//!   plus what the employer adds. Nothing here applies a rate, so the ledger can never disagree with the payslips it is
//!   recording.
//! * **Four facts, four lines.** The work cost the business the gross wages. What the employer adds on top - its own NSSA and
//!   pension contribution - is a further cost of employing somebody, not part of their pay. The staff are owed the net. And
//!   the business holds everything withheld, plus the employer's own contributions, for somebody else, so it owes that
//!   onward. Debits are the two costs, credits are the two things owed, and they are equal by construction.
//! * **One deductions account, every item named.** A Book numbers its own chart, so which liability each statutory item
//!   belongs to is the bookkeeper's decision, not this journal's. The entry carries one line for everything withheld, with
//!   each item named and its figure in the line's own note, and the response repeats the breakdown so nothing is hidden by
//!   the grouping.
//! * **A closed month is not written into**, and a month is posted once: the reference is the person's own, so the ledger
//!   refuses a second run carrying it.
//! * **No money moves.** Vimbai records what happened. Wages that were actually paid are recorded in the cash book, in the
//!   same way as any other money leaving an account; this journal only says what the month cost and what it owes.
//!
//! Paying the staff, paying the revenue authority and paying the pension fund are three later cash book payments against the
//! two liabilities this posts. They are not this journal's business, and doing them here would be Vimbai paying people.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money, money_or_nothing, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, PAYROLL_RUN};

use crate::ledger_book;
use crate::periods;
use crate::subsidiary_journals::post_lines;

/// The accounts a run posts to, as the Book's own chart numbers them.
struct Accounts {
    /// What the work cost: wages, and the employer's own contributions when the chart keeps them apart.
    wages: String,
    contributions: Option<String>,
    /// What the business holds for somebody else: tax withheld, social security, pension, medical aid, a garnishee.
    deductions: String,
    /// What the staff are still owed.
    net: String,
}

/// What the chart calls an account: its own records say `name`, a ledger view says `account_name`, and both are one fact.
fn account_name(account: &Value) -> String {
    let named = field(account, "account_name");
    if named.trim().is_empty() {
        field(account, "name").to_string()
    } else {
        named.to_string()
    }
}

/// An account the caller named, or the first one the chart names this way.
///
/// `called` is a list of words any one of which names the account, because a chart may call the same thing "Salaries and
/// Wages", "Wages" or "Payroll". A caller that names an account wins, and a number the chart does not hold is refused
/// rather than posted to nothing.
fn account_for<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
    key: &str,
    called: &[&str],
    kind: &str,
    required: bool,
) -> Result<Option<String>, StoreError> {
    let accounts = ledger_book::accounts(graph, scope)?;
    let named = text(payload, key);
    if !named.trim().is_empty() {
        let found = accounts
            .iter()
            .any(|account| field(account, "account_number") == named.trim());
        if !found {
            return Err(refused(&format!(
                "Account {named} is not in this Book's chart, so nothing can be posted to it."
            )));
        }
        return Ok(Some(named.trim().to_string()));
    }
    let found = accounts
        .iter()
        .filter(|account| field(account, "account_type") == kind)
        .find(|account| {
            let name = account_name(account).to_lowercase();
            called.iter().any(|word| name.contains(word))
        })
        .map(|account| field(account, "account_number").to_string());
    match (found, required) {
        (Some(number), _) => Ok(Some(number)),
        (None, false) => Ok(None),
        (None, true) => Err(refused(&format!(
            "This Book has no account for {}, so say which account it belongs in ({key}).",
            called.join(" or ")
        ))),
    }
}

fn accounts_for<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Accounts, StoreError> {
    let wages = account_for(
        graph,
        scope,
        payload,
        "wages_account",
        &["salar", "wage"],
        "expense",
        true,
    )?
    .unwrap_or_default();
    let deductions = account_for(
        graph,
        scope,
        payload,
        "deductions_account",
        &["statutory", "deduction", "paye"],
        "liability",
        true,
    )?
    .unwrap_or_default();
    let net = account_for(
        graph,
        scope,
        payload,
        "net_account",
        &["net pay", "salar", "wage"],
        "liability",
        true,
    )?
    .unwrap_or_default();
    // employer contributions get their own cost when the chart has one, and ride on wages when it does not: a Book that
    // keeps one account for the cost of employing people is not wrong, it is simpler
    let contributions = account_for(
        graph,
        scope,
        payload,
        "contributions_account",
        &["employer", "contribution", "nssa", "pension"],
        "expense",
        false,
    )?;
    if wages == deductions || wages == net || deductions == net {
        return Err(refused(
            "What the work cost, what the staff are owed and what is held for somebody else are three different facts, so they cannot share one account.",
        ));
    }
    // the contributions cost is only a separate line when it is a different account from wages
    let contributions = contributions.filter(|account| *account != wages);
    Ok(Accounts {
        wages,
        contributions,
        deductions,
        net,
    })
}

/// A figure on a payslip cannot be negative: a deduction is money withheld, and the employer's contribution is money added.
fn at_least_nothing(payload: &Value, key: &str, what: &str) -> Result<Money, StoreError> {
    let amount = money_or_nothing(payload, key)?;
    if amount.is_negative() {
        return Err(refused(&format!(
            "{what} cannot be less than nothing: a deduction is money withheld, not money added."
        )));
    }
    Ok(amount)
}

/// What was withheld, item by item, so the entry's note can name each one rather than one unexplained figure.
fn withheld(payload: &Value) -> Result<Vec<(&'static str, Money)>, StoreError> {
    Ok(vec![
        (
            "PAYE",
            at_least_nothing(payload, "paye", "The PAYE withheld")?,
        ),
        (
            "NSSA",
            at_least_nothing(payload, "nassa", "The NSSA withheld")?,
        ),
        (
            "pension",
            at_least_nothing(payload, "pension", "The pension withheld")?,
        ),
        (
            "medical aid",
            at_least_nothing(payload, "medical_aid", "The medical aid")?,
        ),
        (
            "other deductions",
            at_least_nothing(payload, "other_deductions", "The other deductions")?,
        ),
    ])
}

/// The items that are not nothing, said as "PAYE 2,500.00, NSSA 450.00", so one line still reads as what it is made of.
fn spoken(items: &[(&str, Money)]) -> String {
    items
        .iter()
        .filter(|(_, amount)| !amount.is_zero())
        .map(|(what, amount)| format!("{what} {}", amount.to_fixed(2)))
        .collect::<Vec<String>>()
        .join(", ")
}

/// `POST /payroll`: post a month's wages, the employer's own contributions, what the staff are owed and what is held for
/// somebody else.
pub fn post_run(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let stated = text(payload, "period");
    let Some(period) = periods::period_of(&stated) else {
        return Err(refused(
            "Say which month the wages are for, as a period such as 2026-11.",
        ));
    };
    let Some(entry_date) = periods::last_day_of(&period) else {
        return Err(refused("That period does not read as a month."));
    };
    // a closed month is not written into: the cost belongs in the month that is still open
    if let Some(refusal) = periods::refusal_for(graph, scope, &entry_date)? {
        return Err(refused(&refusal));
    }
    let reference = text(payload, "reference");
    if reference.trim().is_empty() {
        return Err(refused(
            "Give the run a reference of your own, so the same month cannot be posted twice.",
        ));
    }
    // the gross may arrive as `gross` or as `gross_salary`, the name the payroll calculator publishes
    let gross = match text(payload, "gross").trim().is_empty() {
        true => money(payload, "gross_salary")?,
        false => money(payload, "gross")?,
    };
    if gross.is_zero() || gross.is_negative() {
        return Err(refused(
            "Say what the month's gross wages were: a run with no wages is not a run.",
        ));
    }
    let items = withheld(payload)?;
    let mut held = Money::zero();
    for (_, amount) in &items {
        held = held.plus(amount);
    }
    let net = gross.minus(&held);
    if net.is_zero() || net.is_negative() {
        return Err(refused(
            "The deductions come to the whole gross or more, so there would be nothing left to pay anybody. Check the figures against the payslips.",
        ));
    }
    let employer = at_least_nothing(
        payload,
        "employer_contributions",
        "The employer's own contributions",
    )?;
    let accounts = accounts_for(graph, scope, payload)?;
    let zero = Money::zero();
    let mut lines: Vec<(String, Money, Money, String)> = Vec::new();
    lines.push((
        accounts.wages.clone(),
        gross.clone(),
        zero.clone(),
        format!("Gross wages for {period}"),
    ));
    if !employer.is_zero() {
        match &accounts.contributions {
            Some(account) => lines.push((
                account.clone(),
                employer.clone(),
                zero.clone(),
                format!("Employer contributions for {period}"),
            )),
            // folded into the wages line, so the cost is still the whole cost of employing people
            None => {
                if let Some(first) = lines.first_mut() {
                    first.1 = gross.plus(&employer);
                    first.3 = format!("Gross wages and employer contributions for {period}");
                }
            }
        }
    }
    let owed_onward = held.plus(&employer);
    if !owed_onward.is_zero() {
        lines.push((
            accounts.deductions.clone(),
            zero.clone(),
            owed_onward.clone(),
            format!("Held for others for {period}: {}", spoken(&items)),
        ));
    }
    lines.push((
        accounts.net.clone(),
        zero,
        net.clone(),
        format!("Owed to staff for {period}"),
    ));
    let what = text(payload, "notes");
    let description = if what.trim().is_empty() {
        format!("Payroll for {period}")
    } else {
        format!("Payroll for {period}: {what}")
    };
    // dated in the month whose profit it changes, not on the day somebody pressed the button
    let dated = json!({"entry_date": entry_date, "created_at": text(payload, "created_at")});
    let entry_id = post_lines(
        graph,
        scope,
        &dated,
        description,
        reference.trim(),
        "Payroll",
        &lines,
    )?;
    let total_cost = gross.plus(&employer);
    let run = json!({
        "period": period,
        "reference_number": reference.trim(),
        "gross_wages": wire(&gross),
        "paye": wire(&items[0].1),
        "nassa": wire(&items[1].1),
        "pension": wire(&items[2].1),
        "medical_aid": wire(&items[3].1),
        "other_deductions": wire(&items[4].1),
        "withheld": wire(&held),
        "employer_contributions": wire(&employer),
        "net_pay": wire(&net),
        "total_cost": wire(&total_cost),
        "wages_account": accounts.wages,
        "deductions_account": accounts.deductions,
        "net_account": accounts.net,
        "journal_entry_id": entry_id,
        "entry_date": entry_date,
    });
    remember(graph, scope, PAYROLL_RUN, run)
}

/// `GET /payroll`: the runs this Book has posted, newest month first, for one month when asked.
pub fn list_runs<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let period = text(query, "period");
    let mut rows: Vec<Value> = Records::new(graph, PAYROLL_RUN)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| period.trim().is_empty() || field(row, "period") == period.trim())
        .collect();
    rows.sort_by(|a, b| {
        field(b, "period")
            .cmp(field(a, "period"))
            .then_with(|| field(a, "reference_number").cmp(field(b, "reference_number")))
    });
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use serde_json::json;
    use vimbai_store::fake::MemoryGraph;
    use vimbai_store::LEDGER_ACCOUNT;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A business Book: its opening chart already holds the accounts a payroll run needs.
    fn business() -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = scope();
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        (graph, scope)
    }

    /// The accounts a posted entry moved, in the order they were written.
    fn lines_of(graph: &MemoryGraph, scope: &BookScope, entry_id: &str) -> Vec<Value> {
        let entry = crate::ledger_book::get_entry(graph, scope, entry_id).expect("the entry");
        entry
            .get("lines")
            .and_then(|lines| lines.as_array())
            .cloned()
            .unwrap_or_default()
    }

    /// What an account was moved by, read from the entry itself rather than from a balance.
    fn moved(lines: &[Value], account: &str, side: &str) -> Money {
        let mut total = Money::zero();
        for line in lines {
            if field(line, "account_number") == account {
                total = total.plus(&money(line, side).expect("a figure"));
            }
        }
        total
    }

    fn balances(lines: &[Value]) -> bool {
        let mut debits = Money::zero();
        let mut credits = Money::zero();
        for line in lines {
            debits = debits.plus(&money(line, "debit").expect("a debit"));
            credits = credits.plus(&money(line, "credit").expect("a credit"));
        }
        debits == credits
    }

    #[test]
    fn a_month_of_wages_posts_what_it_cost_and_what_it_owes() {
        let (graph, scope) = business();
        let posted = post_run(
            &graph,
            &scope,
            &json!({
                "period": "2026-11",
                "reference": "PAY-2026-11",
                "gross": "10000.00",
                "paye": "2500.00",
                "nassa": "450.00",
            }),
        )
        .expect("a posted run");

        // the figures the month carries, stated back exactly as they were worked out
        assert_eq!(field(&posted, "gross_wages"), "10000.000000");
        assert_eq!(field(&posted, "net_pay"), "7050.000000");
        assert_eq!(field(&posted, "withheld"), "2950.000000");
        assert_eq!(field(&posted, "total_cost"), "10000.000000");
        assert_eq!(field(&posted, "period"), "2026-11");

        let lines = lines_of(&graph, &scope, field(&posted, "journal_entry_id"));
        assert!(balances(&lines), "a payroll entry must balance");
        // what the work cost, what the staff are owed, what is held for the tax authority
        assert_eq!(
            moved(&lines, "6250", "debit"),
            Money::from_str("10000.00").unwrap()
        );
        assert_eq!(
            moved(&lines, "2250", "credit"),
            Money::from_str("7050.00").unwrap()
        );
        assert_eq!(
            moved(&lines, "2260", "credit"),
            Money::from_str("2950.00").unwrap()
        );
        // the entry is dated in the month whose profit it changes, not on the day it was pressed
        assert_eq!(field(&posted, "entry_date"), "2026-11-30");
    }

    #[test]
    fn the_employers_own_contributions_are_a_cost_and_are_owed_onward() {
        let (graph, scope) = business();
        let posted = post_run(
            &graph,
            &scope,
            &json!({
                "period": "2026-11",
                "reference": "PAY-2026-11",
                "gross_salary": "10000.00",
                "paye": "2500.00",
                "nassa": "450.00",
                "employer_contributions": "1000.00",
            }),
        )
        .expect("a posted run");

        // the cost of employing people is the wages plus what the employer adds, not the wages alone
        assert_eq!(field(&posted, "total_cost"), "11000.000000");
        assert_eq!(field(&posted, "employer_contributions"), "1000.000000");

        let lines = lines_of(&graph, &scope, field(&posted, "journal_entry_id"));
        assert!(balances(&lines));
        assert_eq!(
            moved(&lines, "6250", "debit"),
            Money::from_str("10000.00").unwrap()
        );
        assert_eq!(
            moved(&lines, "6260", "debit"),
            Money::from_str("1000.00").unwrap()
        );
        assert_eq!(
            moved(&lines, "2250", "credit"),
            Money::from_str("7050.00").unwrap()
        );
        // what is held for somebody else is the withheld money plus the employer's own contribution
        assert_eq!(
            moved(&lines, "2260", "credit"),
            Money::from_str("3950.00").unwrap()
        );
    }

    #[test]
    fn each_item_withheld_is_named_on_the_line_it_rides_on() {
        let (graph, scope) = business();
        let posted = post_run(
            &graph,
            &scope,
            &json!({
                "period": "2026-11",
                "reference": "PAY-2026-11",
                "gross": "10000.00",
                "paye": "2500.00",
                "pension": "500.00",
                "medical_aid": "300.00",
            }),
        )
        .expect("a posted run");
        let lines = lines_of(&graph, &scope, field(&posted, "journal_entry_id"));
        let note = lines
            .iter()
            .find(|line| field(line, "account_number") == "2260")
            .map(|line| field(line, "description"))
            .expect("the deductions line");
        // one line, but nothing about it is hidden: each item and its figure are named
        assert!(note.contains("PAYE 2500.00"), "{note}");
        assert!(note.contains("pension 500.00"), "{note}");
        assert!(note.contains("medical aid 300.00"), "{note}");
        // an item that was nothing is not named, because naming it would suggest it was withheld
        assert!(!note.contains("NSSA"), "{note}");
    }

    #[test]
    fn a_book_that_keeps_one_account_for_the_cost_of_employing_people_folds_the_contributions_in() {
        let (graph, scope) = business();
        // a chart with the wages account and no separate one for what the employer adds
        let chart = crate::ledger_book::accounts(&graph, &scope).expect("a chart");
        assert!(chart
            .iter()
            .any(|account| field(account, "account_number") == "6260"));
        crate::ledger_book::delete_account(&graph, &scope, "6260").expect("removed");

        let posted = post_run(
            &graph,
            &scope,
            &json!({
                "period": "2026-11",
                "reference": "PAY-2026-11",
                "gross": "10000.00",
                "paye": "2500.00",
                "employer_contributions": "1000.00",
            }),
        )
        .expect("a posted run");
        let lines = lines_of(&graph, &scope, field(&posted, "journal_entry_id"));
        assert!(
            balances(&lines),
            "the cost is still the whole cost of employing people"
        );
        // the whole cost of employing people lands on the wages line, and the response still says what the split was
        assert_eq!(
            moved(&lines, "6250", "debit"),
            Money::from_str("11000.00").unwrap()
        );
        assert_eq!(field(&posted, "employer_contributions"), "1000.000000");
        assert_eq!(field(&posted, "total_cost"), "11000.000000");
    }

    #[test]
    fn deductions_that_swallow_the_whole_gross_are_refused() {
        let (graph, scope) = business();
        let refused = post_run(
            &graph,
            &scope,
            &json!({
                "period": "2026-11",
                "reference": "PAY-2026-11",
                "gross": "1000.00",
                "paye": "800.00",
                "nassa": "200.00",
            }),
        )
        .expect_err("nothing would be left to pay anybody");
        assert!(
            format!("{refused}").contains("nothing left to pay anybody"),
            "{refused}"
        );
    }

    #[test]
    fn a_deduction_cannot_be_negative() {
        let (graph, scope) = business();
        let refused = post_run(
            &graph,
            &scope,
            &json!({
                "period": "2026-11",
                "reference": "PAY-2026-11",
                "gross": "1000.00",
                "paye": "-50.00",
            }),
        )
        .expect_err("a deduction is money withheld");
        assert!(
            format!("{refused}").contains("cannot be less than nothing"),
            "{refused}"
        );
    }

    #[test]
    fn a_closed_month_is_not_written_into() {
        let (graph, scope) = business();
        crate::periods::close_period(
            &graph,
            &scope,
            &json!({"period": "2026-11", "closes_on": "2026-11-18"}),
        )
        .expect("the month is closed");
        let refused = post_run(
            &graph,
            &scope,
            &json!({
                "period": "2026-11",
                "reference": "PAY-2026-11",
                "gross": "1000.00",
            }),
        )
        .expect_err("a closed month is not written into");
        assert!(
            format!("{refused}").to_lowercase().contains("closed"),
            "{refused}"
        );
    }

    #[test]
    fn the_same_run_cannot_be_posted_twice() {
        let (graph, scope) = business();
        let payload = json!({
            "period": "2026-11",
            "reference": "PAY-2026-11",
            "gross": "10000.00",
            "paye": "2500.00",
        });
        post_run(&graph, &scope, &payload).expect("the first run");
        let refused = post_run(&graph, &scope, &payload).expect_err("a reference is posted once");
        assert!(
            format!("{refused}")
                .to_lowercase()
                .contains("already exists"),
            "{refused}"
        );
    }

    #[test]
    fn a_book_with_no_wages_account_is_told_which_account_to_name() {
        let graph = MemoryGraph::new();
        let scope = scope();
        // a chart of nothing: a run has nowhere to put the cost
        let _ = Records::new(&graph, LEDGER_ACCOUNT);
        let refused = post_run(
            &graph,
            &scope,
            &json!({
                "period": "2026-11",
                "reference": "PAY-2026-11",
                "gross": "1000.00",
            }),
        )
        .expect_err("nowhere to post the cost");
        assert!(format!("{refused}").contains("wages_account"), "{refused}");
    }

    #[test]
    fn the_runs_this_book_posted_are_listed_newest_month_first() {
        let (graph, scope) = business();
        for (period, reference) in [("2026-10", "PAY-10"), ("2026-11", "PAY-11")] {
            post_run(
                &graph,
                &scope,
                &json!({
                    "period": period,
                    "reference": reference,
                    "gross": "5000.00",
                    "paye": "1000.00",
                }),
            )
            .expect("a posted run");
        }
        let listed = list_runs(&graph, &scope, &json!({})).expect("the runs");
        let periods: Vec<String> = listed
            .as_array()
            .expect("a list")
            .iter()
            .map(|row| field(row, "period").to_string())
            .collect();
        assert_eq!(periods, vec!["2026-11".to_string(), "2026-10".to_string()]);
        // the month asked for is the only one answered
        let one = list_runs(&graph, &scope, &json!({"period": "2026-10"})).expect("the run");
        assert_eq!(one.as_array().expect("a list").len(), 1);
    }
}
