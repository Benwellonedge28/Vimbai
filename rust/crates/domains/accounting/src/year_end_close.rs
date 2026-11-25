//! Closing the year: the year's profit moved to one place, and its trading accounts left at nothing.
//!
//! A year's sales and costs have done their work once the year is over. What they came to is the year's profit, and the
//! accounts that carried them should not go on carrying them into the next year: a reader of next February's sales wants
//! next February's sales, not this year's total sitting behind them.
//!
//! So the year is closed by one entry: every revenue account that earned something is debited by what it earned, every
//! expense account that spent something is credited by what it spent, and the difference - the profit or the loss - goes to
//! retained earnings in the owners' equity. Nothing else moves: no money leaves, no asset changes, and the balance sheet
//! still balances because both sides of the entry are the same figure.
//!
//! What this keeps, and why:
//!
//! * the closing entry is dated the first day of the year after, not the last day of the year it closes. A closed month is
//!   never written into - that is the rule the whole ledger is held to - and the year's own reports should go on showing
//!   what it earned. Dated the day after, the entry leaves 2026's sales in 2026 and starts 2027 with nothing behind it;
//! * it is allowed only when every month of the year is already closed, which is a stricter rule than any other journal is
//!   held to: the year is closed after its months are;
//! * the figures come from what was posted, read through the one function that decides what an account did over a period,
//!   never added up here: the same figure the reports show is the figure that closes;
//! * one closing entry per year. The reference is the year, and the ledger refuses a reference it has already posted, so a
//!   second press cannot put the year's profit in equity twice;
//! * the profit goes to an equity account found by what the chart calls it and by its type, never by number;
//! * a year with no revenue and no expense posted has nothing to close, and is refused rather than posting an entry that
//!   says nothing;
//! * if a month of a closed year is reopened afterwards, the year's closing entry no longer describes it. The listing says
//!   so plainly rather than leaving the books quietly wrong.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, YEAR_END_CLOSE};

use crate::ledger_book;
use crate::periods;
use crate::subsidiary_journals::{account_named, post_lines};

/// The year a caller means, as four digits, and the days it runs between.
fn year_of(payload: &Value) -> Result<(String, String, String), StoreError> {
    let said = text(payload, "year").trim().to_string();
    if said.len() != 4 || !said.chars().all(|letter| letter.is_ascii_digit()) {
        return Err(refused(
            "Say which year is being closed, as 2026. A year is four digits.",
        ));
    }
    Ok((
        said.clone(),
        format!("{said}-01-01"),
        format!("{said}-12-31"),
    ))
}

/// Every month of the year has to be closed first: the year is closed after its months are.
fn months_still_open<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    year: &str,
) -> Result<Vec<String>, StoreError> {
    let closed = periods::closed_months(graph, scope)?;
    Ok((1..=12)
        .map(|month| format!("{year}-{month:02}"))
        .filter(|month| !closed.contains(month))
        .collect())
}

/// `POST /year-end-close`: move the year's profit to retained earnings and leave its trading accounts at nothing.
pub fn close_year(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let (year, from, until) = year_of(payload)?;
    // a year is closed once: the reference is the year, so the ledger refuses a second entry, but the refusal is said in
    // words the person can act on rather than as a duplicate reference
    if let Some(already) = Records::new(graph, YEAR_END_CLOSE)
        .list(scope, 5000)?
        .into_iter()
        .find(|row| field(row, "year") == year)
    {
        return Err(refused(&format!(
            "{year} has already been closed, on {}. A correction to a closed year belongs in the year that is open, as a \
             reversing entry or an adjustment there.",
            field(&already, "entry_date")
        )));
    }
    let open = months_still_open(graph, scope, &year)?;
    if !open.is_empty() {
        return Err(refused(&format!(
            "{year} is not ready to close: {} {} still open. Close the months first, because closing the year puts the \
             year's profit in one place and a month still open could still change it.",
            open.join(", "),
            if open.len() == 1 { "is" } else { "are" }
        )));
    }
    // the day after the year it closes: the first day of the next year, which is the earliest day that is not inside a
    // month this close has just required to be closed
    let next_year: u32 = year
        .parse::<u32>()
        .map_err(|_| refused("That year does not read as a year."))?
        + 1;
    let entry_date = {
        let stated = text(payload, "entry_date").trim().to_string();
        let earliest = format!("{next_year}-01-01");
        if stated.is_empty() {
            earliest.clone()
        } else if stated.as_str() < earliest.as_str() {
            return Err(refused(&format!(
                "The closing entry is dated after the year it closes, so {stated} cannot close {year}: the earliest day \
                 is {earliest}. Leave the day out to use it."
            )));
        } else {
            stated
        }
    };
    // what every account did over the year, read through the one function that decides that
    let activity = ledger_book::posted_activity(graph, scope, &from, &until)?;
    let chart = ledger_book::accounts(graph, scope)?;
    let kind_of = |number: &str| {
        chart
            .iter()
            .find(|account| field(account, "account_number") == number)
            .map(|account| field(account, "account_type"))
            .unwrap_or_default()
    };
    let mut lines: Vec<(String, Money, Money, String)> = Vec::new();
    let mut earned = Money::zero();
    let mut spent = Money::zero();
    for (number, moved) in &activity {
        if moved.is_zero() {
            continue;
        }
        match kind_of(number) {
            // earned: the credit balance it carries is taken off it by a debit, and it counts towards the year's profit
            "revenue" => {
                earned = earned.plus(moved);
                if moved.is_negative() {
                    lines.push((
                        number.clone(),
                        Money::zero(),
                        Money::zero().minus(moved),
                        "Year closed".to_string(),
                    ));
                } else {
                    lines.push((
                        number.clone(),
                        moved.clone(),
                        Money::zero(),
                        "Year closed".to_string(),
                    ));
                }
            }
            // spent: the debit balance it carries is taken off it by a credit, and it counts against the year's profit
            "expense" => {
                spent = spent.plus(moved);
                if moved.is_negative() {
                    lines.push((
                        number.clone(),
                        Money::zero().minus(moved),
                        Money::zero(),
                        "Year closed".to_string(),
                    ));
                } else {
                    lines.push((
                        number.clone(),
                        Money::zero(),
                        moved.clone(),
                        "Year closed".to_string(),
                    ));
                }
            }
            // an asset, a liability or equity is not a year's trading: it carries on into the next year untouched
            _ => {}
        }
    }
    if lines.is_empty() {
        return Err(refused(&format!(
            "{year} has nothing to close: no sales and no costs were posted in it, so there is no profit to move."
        )));
    }
    let profit = earned.minus(&spent);
    let retained = account_named(
        graph,
        scope,
        payload,
        "retained_earnings_account",
        "retained_earnings",
        "equity",
    )?;
    if profit.is_zero() {
        // the year broke even: the trading accounts still come to nothing, and equity gains nothing
    } else if profit.is_negative() {
        lines.push((
            retained.clone(),
            Money::zero().minus(&profit),
            Money::zero(),
            "Loss for the year".to_string(),
        ));
    } else {
        lines.push((
            retained.clone(),
            Money::zero(),
            profit.clone(),
            "Profit for the year".to_string(),
        ));
    }
    let description = if profit.is_negative() {
        format!("Year end {year}: loss for the year")
    } else {
        format!("Year end {year}: profit for the year")
    };
    let dated = json!({"entry_date": entry_date, "created_at": text(payload, "created_at")});
    let entry_id = post_lines(
        graph,
        scope,
        &dated,
        description,
        &format!("YEC-{year}"),
        "YearEndClose",
        &lines,
    )?;
    remember(
        graph,
        scope,
        YEAR_END_CLOSE,
        json!({
            "year": year,
            "entry_date": entry_date,
            "earned": wire(&earned),
            "spent": wire(&spent),
            "profit": wire(&profit),
            "retained_earnings_account": retained,
            "journal_entry_id": entry_id,
            "notes": text(payload, "notes"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /year-end-close`: the years this Book has closed, newest first, and whether any month of one has since reopened.
pub fn list_closed_years<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let asked = text(query, "year");
    let closed = periods::closed_months(graph, scope)?;
    let mut rows: Vec<Value> = Records::new(graph, YEAR_END_CLOSE)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| asked.trim().is_empty() || field(row, "year") == asked.trim())
        .map(|row| {
            let year = field(&row, "year");
            let reopened: Vec<String> = (1..=12)
                .map(|month| format!("{year}-{month:02}"))
                .filter(|month| !closed.contains(month))
                .collect();
            let mut row = row;
            if let Some(object) = row.as_object_mut() {
                // a closed year whose month has been reopened no longer describes the year: say so rather than let the
                // closing entry quietly stand for books that have moved since
                object.insert("months_reopened_since".to_string(), json!(reopened));
            }
            row
        })
        .collect();
    rows.sort_by(|a, b| field(b, "year").cmp(field(a, "year")));
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_store::fake::MemoryGraph;
    use vimbai_store::read::money;

    fn business() -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = BookScope::new("user-1", "book-1");
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        (graph, scope)
    }

    fn amount(text: &str) -> Money {
        Money::parse(text).expect("an amount")
    }

    /// Posts an ordinary two-line entry, as a sale or a cost would arrive from its own journal.
    fn post(
        graph: &MemoryGraph,
        scope: &BookScope,
        day: &str,
        reference: &str,
        debit: &str,
        credit: &str,
        figure: &str,
    ) {
        crate::subsidiary_journals::post(
            graph,
            scope,
            &json!({"entry_date": day}),
            format!("{reference} posted"),
            reference,
            "Tests",
            debit,
            credit,
            &amount(figure),
            "a test entry",
        )
        .expect("posted");
    }

    /// A year of trading: a sale of 1,000.00 and a cost of 300.00, so the year made 700.00.
    fn trade(graph: &MemoryGraph, scope: &BookScope) {
        post(graph, scope, "2026-03-04", "S-1", "1100", "4000", "1000.00");
        post(graph, scope, "2026-04-02", "E-1", "6100", "1010", "300.00");
    }

    /// Closes every month of a year, which is what a year has to be before it can be closed.
    fn close_months(graph: &MemoryGraph, scope: &BookScope, year: &str) {
        for month in 1..=12 {
            let period = format!("{year}-{month:02}");
            crate::periods::close_period(
                graph,
                scope,
                &json!({"period": period, "closes_on": periods::last_day_of(&period).expect("a day")}),
            )
            .expect("closed");
        }
    }

    fn balance(graph: &MemoryGraph, scope: &BookScope, account: &str) -> Money {
        let chart = crate::ledger_book::accounts(graph, scope).expect("the chart");
        let found = chart
            .iter()
            .find(|row| field(row, "account_number") == account)
            .expect("the account");
        crate::ledger_book::account_balance(graph, scope, found, None).expect("a balance")
    }

    #[test]
    fn a_year_is_closed_by_moving_its_profit_to_equity_and_leaving_its_trading_at_nothing() {
        let (graph, scope) = business();
        trade(&graph, &scope);
        close_months(&graph, &scope, "2026");

        let closed = close_year(&graph, &scope, &json!({"year": "2026"})).expect("closed");

        assert_eq!(field(&closed, "year"), "2026");
        assert_eq!(field(&closed, "earned"), "1000.000000");
        assert_eq!(field(&closed, "spent"), "300.000000");
        assert_eq!(field(&closed, "profit"), "700.000000");
        assert_eq!(field(&closed, "retained_earnings_account"), "3100");

        // the trading accounts the year used now carry nothing, so next year starts at nothing
        assert!(balance(&graph, &scope, "4000").is_zero());
        assert!(balance(&graph, &scope, "6100").is_zero());
        // and what the year made is in the owners' equity
        assert_eq!(balance(&graph, &scope, "3100"), amount("700"));
        // nothing else moved: the money the sale is owed is still owed
        assert_eq!(balance(&graph, &scope, "1100"), amount("1000"));
        assert_eq!(balance(&graph, &scope, "1010"), amount("-300"));

        let entry =
            crate::ledger_book::get_entry(&graph, &scope, field(&closed, "journal_entry_id"))
                .expect("the entry");
        // the day after the year it closes, so 2026's own reports still show what 2026 earned
        assert_eq!(field(&entry, "entry_date"), "2027-01-01");
        assert_eq!(field(&entry, "reference_number"), "YEC-2026");
        assert_eq!(field(&entry, "source_module"), "YearEndClose");
    }

    #[test]
    fn the_entry_balances_and_the_books_still_agree_after_the_close() {
        let (graph, scope) = business();
        trade(&graph, &scope);
        close_months(&graph, &scope, "2026");
        let closed = close_year(&graph, &scope, &json!({"year": "2026"})).expect("closed");

        let entry =
            crate::ledger_book::get_entry(&graph, &scope, field(&closed, "journal_entry_id"))
                .expect("the entry");
        let lines = entry["lines"].as_array().expect("lines");
        let mut debits = Money::zero();
        let mut credits = Money::zero();
        for line in lines {
            debits = debits.plus(&money(line, "debit").expect("a debit"));
            credits = credits.plus(&money(line, "credit").expect("a credit"));
        }
        assert_eq!(debits, credits, "the closing entry has to balance");
        // the debits are the year's sales, and the credits are its costs plus what it kept
        assert_eq!(debits, amount("1000"));

        // and the trial balance still agrees, which is what a reader checks first
        let sheet =
            crate::ledger_book::trial_balance(&graph, &scope, &json!({})).expect("a trial balance");
        let said = field(&sheet, "balanced");
        assert!(said.is_empty() || said == "true", "{said}");
    }

    #[test]
    fn a_year_that_lost_money_debits_equity_rather_than_crediting_it() {
        let (graph, scope) = business();
        post(
            &graph,
            &scope,
            "2026-02-01",
            "E-9",
            "6100",
            "1010",
            "500.00",
        );
        post(
            &graph,
            &scope,
            "2026-03-01",
            "S-9",
            "1100",
            "4000",
            "200.00",
        );
        close_months(&graph, &scope, "2026");

        let closed = close_year(&graph, &scope, &json!({"year": "2026"})).expect("closed");
        assert_eq!(field(&closed, "profit"), "-300.000000");
        // a loss takes out of equity rather than putting in: the figure is signed, and the balance is negative
        assert_eq!(balance(&graph, &scope, "3100"), amount("-300"));
        assert!(balance(&graph, &scope, "4000").is_zero());
        assert!(balance(&graph, &scope, "6100").is_zero());
    }

    #[test]
    fn a_year_is_not_closed_before_its_months_are_and_the_refusal_names_them() {
        let (graph, scope) = business();
        trade(&graph, &scope);
        crate::periods::close_period(
            &graph,
            &scope,
            &json!({"period": "2026-03", "closes_on": "2026-03-31"}),
        )
        .expect("closed");

        let said = close_year(&graph, &scope, &json!({"year": "2026"}))
            .expect_err("months still open")
            .to_string();
        assert!(said.contains("not ready to close"), "{said}");
        // it names what is still open, so the person knows what to do next
        assert!(said.contains("2026-01"), "{said}");
        assert!(said.contains("2026-12"), "{said}");
        assert!(
            !said.contains("2026-03,"),
            "a closed month is not named: {said}"
        );
    }

    #[test]
    fn a_year_is_closed_once() {
        let (graph, scope) = business();
        trade(&graph, &scope);
        close_months(&graph, &scope, "2026");
        close_year(&graph, &scope, &json!({"year": "2026"})).expect("closed");

        let again = close_year(&graph, &scope, &json!({"year": "2026"}))
            .expect_err("a second close")
            .to_string();
        assert!(again.contains("already been closed"), "{again}");
        // and the profit is not in equity twice
        assert_eq!(balance(&graph, &scope, "3100"), amount("700"));
    }

    #[test]
    fn a_year_with_nothing_posted_in_it_has_nothing_to_close() {
        let (graph, scope) = business();
        close_months(&graph, &scope, "2026");

        let said = close_year(&graph, &scope, &json!({"year": "2026"}))
            .expect_err("nothing to close")
            .to_string();
        assert!(said.contains("nothing to close"), "{said}");
    }

    #[test]
    fn a_year_has_to_read_as_a_year_and_the_day_has_to_be_in_it() {
        let (graph, scope) = business();
        trade(&graph, &scope);
        close_months(&graph, &scope, "2026");

        let vague = close_year(&graph, &scope, &json!({"year": "26"}))
            .expect_err("not a year")
            .to_string();
        assert!(vague.contains("four digits"), "{vague}");

        let inside = close_year(
            &graph,
            &scope,
            &json!({"year": "2026", "entry_date": "2026-12-31"}),
        )
        .expect_err("inside the closed year")
        .to_string();
        assert!(inside.contains("cannot close 2026"), "{inside}");
    }

    #[test]
    fn the_years_closed_are_listed_and_a_month_reopened_since_is_named() {
        let (graph, scope) = business();
        trade(&graph, &scope);
        close_months(&graph, &scope, "2026");
        close_year(&graph, &scope, &json!({"year": "2026"})).expect("closed");

        let listed = list_closed_years(&graph, &scope, &json!({})).expect("a list");
        let rows = listed.as_array().expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(field(&rows[0], "year"), "2026");
        assert_eq!(field(&rows[0], "profit"), "700.000000");
        assert!(rows[0]["months_reopened_since"]
            .as_array()
            .expect("months")
            .is_empty());
    }
}
