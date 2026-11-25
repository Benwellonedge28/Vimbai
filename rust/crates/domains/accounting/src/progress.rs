#![forbid(unsafe_code)]
//! How the business is doing, to any day, read from the ledger.
//!
//! A month is how figures are locked, not how they are seen. Every figure here is a view over a window that ends
//! wherever the person is standing - today, the fifth, the end of a quarter - so nobody has to wait for a month to close
//! before they know whether the business is making money. The window is a window, not a month: it opens where the caller
//! says and closes where the caller says.
//!
//! Three things this reader will not do:
//!
//! * it will not annualise, project or forecast. A projection dressed as a figure is how a person makes a decision on
//!   numbers that do not exist yet.
//! * it will not compare a part of a month with a whole one. Comparing 25 days with 30 always lies, so the comparison is
//!   like for like: the same days of the previous month when the window starts on the first, and otherwise the same
//!   number of days immediately before it.
//! * it will not hide what has not been posted. Profit is provisional while entries sit unposted, so the count of those
//!   entries travels with the figure rather than being left to be discovered later.
//!
//! Nothing here names an account. Revenue and costs are the Book's own revenue and expense accounts, cash is the money
//! accounts the Book keeps, what is owed to and by the business is the outstanding documents it holds, and tax is the
//! tax ledger's closing figure. A Book that calls its accounts something else still reads correctly, because the words
//! are the Book's and never this file's.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, refused};
use vimbai_store::{BookScope, Graph, StoreError};

use crate::cashbook;
use crate::ledger_book;
use crate::subsidiary_journals;
use crate::tax_ledger;

/// How a day is written. A window that cannot be read is refused in words rather than guessed at.
fn day(text: &str, what: &str) -> Result<(i32, u32, u32), StoreError> {
    let parts: Vec<&str> = text.split('-').collect();
    let wrong = || {
        refused(&format!(
            "Say {what} as a day, 2026-10-20, rather than \"{text}\"."
        ))
    };
    if parts.len() != 3 {
        return Err(wrong());
    }
    let year: i32 = parts[0].parse().map_err(|_| wrong())?;
    let month: u32 = parts[1].parse().map_err(|_| wrong())?;
    let day_of_month: u32 = parts[2].parse().map_err(|_| wrong())?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day_of_month) {
        return Err(wrong());
    }
    if day_of_month > days_in_month(year, month) {
        return Err(refused(&format!(
            "{text} is not a day: {year}-{month:02} has {} days.",
            days_in_month(year, month)
        )));
    }
    Ok((year, month, day_of_month))
}

fn leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn written(year: i32, month: u32, day_of_month: u32) -> String {
    format!("{year:04}-{month:02}-{day_of_month:02}")
}

/// Days since 1970-01-01, so a window can be moved by a number of days exactly.
///
/// Written out rather than pulled in: a date library for two additions would be a dependency to keep current for
/// arithmetic that does not change.
fn days_from_civil(year: i32, month: u32, day_of_month: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let year_of_era = y - era * 400;
    let m = month as i64;
    let day = day_of_month as i64;
    let shifted = if m > 2 { m - 3 } else { m + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let y = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day_of_month = (day_of_year - (153 * shifted + 2) / 5 + 1) as u32;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    } as u32;
    let year = if month <= 2 { y + 1 } else { y } as i32;
    (year, month, day_of_month)
}

fn moved(a: (i32, u32, u32), by: i64) -> (i32, u32, u32) {
    civil_from_days(days_from_civil(a.0, a.1, a.2) + by)
}

/// The window to compare with, like for like.
///
/// A window that starts on the first of a month is compared with the same days of the month before it, because that is
/// the comparison a business makes: the first 25 days against the first 25 of last month. Any other window is compared
/// with the same number of days immediately before it, so the two are always the same length.
fn comparison(from: (i32, u32, u32), to: (i32, u32, u32)) -> (String, String) {
    let length = days_from_civil(to.0, to.1, to.2) - days_from_civil(from.0, from.1, from.2) + 1;
    if from.2 == 1 && (from.0, from.1) == (to.0, to.1) {
        let (year, month) = if from.1 == 1 {
            (from.0 - 1, 12)
        } else {
            (from.0, from.1 - 1)
        };
        let end = to.2.min(days_in_month(year, month));
        return (written(year, month, 1), written(year, month, end));
    }
    let end = moved(from, -1);
    let start = moved(end, -(length - 1));
    (
        written(start.0, start.1, start.2),
        written(end.0, end.1, end.2),
    )
}

/// One statement over a window, as the figures this reader needs.
fn traded(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    from: &str,
    to: &str,
) -> Result<(Money, Money, Money), StoreError> {
    let statement =
        ledger_book::income_statement(graph, scope, &json!({"start_date": from, "end_date": to}))?;
    let amount = |key: &str| -> Result<Money, StoreError> {
        let stated = field(&statement, key);
        Money::parse(stated)
            .map_err(|_| refused(&format!("the statement answered {key} as \"{stated}\"")))
    };
    let net = amount("net_income")?;
    // the statement's net income is revenue less costs, so costs are what is left when profit is taken from revenue
    let revenue = {
        let mut total = Money::zero();
        for row in statement
            .get("revenues")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            total =
                total.plus(&Money::parse(field(row, "amount")).unwrap_or_else(|_| Money::zero()));
        }
        total
    };
    let costs = revenue.minus(&net);
    Ok((revenue, costs, net))
}

/// What the Book's money accounts hold, as the cash book keeps them.
fn money_held(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<(Money, Vec<Value>), StoreError> {
    let answer = cashbook::list_accounts(graph, scope, &json!({"is_active": true}))?;
    let mut total = Money::zero();
    let mut rows = Vec::new();
    for account in answer.as_array().map(Vec::as_slice).unwrap_or(&[]) {
        let held =
            Money::parse(field(account, "current_balance")).unwrap_or_else(|_| Money::zero());
        total = total.plus(&held);
        rows.push(json!({
            "name": field(account, "account_name"),
            "kind": field(account, "account_type"),
            "balance": wire(&held),
        }));
    }
    Ok((total, rows))
}

/// `GET /accounting/progress/`: how the business is doing, to any day.
///
/// The day is required, because a reader that assumed today would answer differently each time it was asked and no two
/// answers could be compared. Everything else follows from it.
pub fn progress<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let to_text = match query.get("to").and_then(Value::as_str) {
        Some(text) if !text.trim().is_empty() => text.trim().to_string(),
        _ => {
            return Err(refused(
                "Say the day to report on, as to=2026-10-20. The day is asked for rather than assumed so that two \
                 readings can be compared.",
            ))
        }
    };
    let to = day(&to_text, "the day to report on")?;
    let from_text = query
        .get("from")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let from = match from_text {
        Some(text) => day(&text, "the day the window opens")?,
        // the month so far, which is the window a person standing in a month means
        None => (to.0, to.1, 1),
    };
    if days_from_civil(from.0, from.1, from.2) > days_from_civil(to.0, to.1, to.2) {
        return Err(refused(&format!(
            "The window opens on {} and closes on {}, which is before it opens.",
            written(from.0, from.1, from.2),
            written(to.0, to.1, to.2)
        )));
    }

    let opened = written(from.0, from.1, from.2);
    let closed = written(to.0, to.1, to.2);
    let (revenue, costs, profit) = traded(graph, scope, &opened, &closed)?;

    let (before_from, before_to) = comparison(from, to);
    let (before_revenue, before_costs, before_profit) =
        traded(graph, scope, &before_from, &before_to)?;

    let (cash, accounts) = money_held(graph, scope)?;
    let owed_to_you = {
        let answer =
            subsidiary_journals::receivables_ledger(graph, scope, &json!({"as_of_date": closed}))?;
        Money::parse(field(&answer, "total_balance_due")).unwrap_or_else(|_| Money::zero())
    };
    let owed_by_you = {
        let answer =
            subsidiary_journals::payables_ledger(graph, scope, &json!({"as_of_date": closed}))?;
        Money::parse(field(&answer, "total_balance_due")).unwrap_or_else(|_| Money::zero())
    };
    let (tax_owed, tax_accounts) = {
        let answer = tax_ledger::tax_ledger(graph, scope, &json!({"from": opened, "to": closed}))?;
        let owed =
            Money::parse(field(&answer["totals"], "closing")).unwrap_or_else(|_| Money::zero());
        let counted = answer
            .get("account_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        (owed, counted)
    };

    // what is recorded but not in the statements, because a profit that hides these is a profit somebody will act on
    let recorded = ledger_book::list_entries(
        graph,
        scope,
        &json!({"start_date": opened, "end_date": closed}),
    )?;
    let mut drafts = 0u64;
    let mut oldest: Option<String> = None;
    for entry in recorded.as_array().map(Vec::as_slice).unwrap_or(&[]) {
        if field(entry, "status") != "draft" {
            continue;
        }
        drafts += 1;
        let date = field(entry, "entry_date").to_string();
        if date.is_empty() {
            continue;
        }
        if oldest
            .as_deref()
            .map(|held| date.as_str() < held)
            .unwrap_or(true)
        {
            oldest = Some(date);
        }
    }

    let margin = if revenue.is_zero() {
        Money::zero()
    } else {
        profit.ratio_pct(&revenue, 2)
    };

    Ok(json!({
        "from": opened,
        "to": closed,
        "days": days_from_civil(to.0, to.1, to.2) - days_from_civil(from.0, from.1, from.2) + 1,
        "revenue": wire(&revenue),
        "costs": wire(&costs),
        "profit": wire(&profit),
        "margin_percent": wire(&margin),
        "compared_with": {
            "from": before_from,
            "to": before_to,
            "revenue": wire(&before_revenue),
            "costs": wire(&before_costs),
            "profit": wire(&before_profit),
        },
        "change": {
            "revenue": wire(&revenue.minus(&before_revenue)),
            "costs": wire(&costs.minus(&before_costs)),
            "profit": wire(&profit.minus(&before_profit)),
        },
        "cash": {"total": wire(&cash), "accounts": accounts},
        "owed_to_you": wire(&owed_to_you),
        "owed_by_you": wire(&owed_by_you),
        "tax": {"owed": wire(&tax_owed), "accounts": tax_accounts},
        "not_posted": {"drafts": drafts, "oldest": oldest},
        "note": if drafts == 0 {
            "Every entry in this window is posted, so the figures are the ledger's own.".to_string()
        } else {
            format!(
                "{drafts} {} in this window {} not been posted, so the profit here is provisional: the figures are \
                 what the ledger holds, not what it will hold.",
                if drafts == 1 { "entry" } else { "entries" },
                if drafts == 1 { "has" } else { "have" },
            )
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashbook;
    use crate::chart::seed_opening_chart;
    use crate::ledger_book::create_entry;
    use crate::subsidiary_journals::{create_purchase, create_sale};
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A business Book that has traded in October: one taxed sale on the 5th and one taxed purchase on the 9th.
    fn traded() -> MemoryGraph {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        create_sale(
            &graph,
            &scope(),
            &json!({"invoice_number": "INV-1", "entry_date": "2026-10-05",
                    "total_amount": "1150.00", "tax_amount": "150.00"}),
        )
        .expect("a taxed sale");
        create_purchase(
            &graph,
            &scope(),
            &json!({"purchase_order_number": "PO-1", "entry_date": "2026-10-09",
                    "total_amount": "460.00", "tax_amount": "60.00"}),
        )
        .expect("a taxed purchase");
        graph
    }

    fn read(graph: &MemoryGraph, query: Value) -> Value {
        progress(graph, &scope(), &query).expect("a reading")
    }

    #[test]
    fn the_month_so_far_reads_the_ledger_rather_than_waiting_for_it_to_close() {
        let answer = read(&traded(), json!({"to": "2026-10-20"}));
        // the month is not over and no month has been closed, but the figures are there
        assert_eq!(answer["from"], json!("2026-10-01"));
        assert_eq!(answer["to"], json!("2026-10-20"));
        assert_eq!(answer["days"], json!(20));
        // the Book's own figures, not figures this reader chose: a sale posts its gross as revenue and its tax as
        // its own cost, and stock bought goes to the stock account rather than to a cost, so the costs here are the
        // tax on the sale alone. Whatever the chart says the costs are, this reads them rather than deciding them.
        assert_eq!(answer["revenue"], json!("1150.000000"));
        assert_eq!(answer["costs"], json!("150.000000"));
        assert_eq!(answer["profit"], json!("1000.000000"));
        assert_eq!(answer["margin_percent"], json!("86.960000"));
    }

    #[test]
    fn a_day_before_anything_was_posted_reads_nothing_rather_than_guessing() {
        let answer = read(&traded(), json!({"to": "2026-10-03"}));
        assert_eq!(answer["revenue"], json!("0.000000"));
        assert_eq!(answer["profit"], json!("0.000000"));
        assert_eq!(answer["margin_percent"], json!("0.000000"));
        // nothing was sold, so there is no margin to state rather than a division by nothing
        assert_eq!(answer["days"], json!(3));
    }

    #[test]
    fn a_part_of_a_month_is_compared_with_the_same_days_of_the_month_before() {
        let answer = read(&traded(), json!({"to": "2026-10-20"}));
        // the first twenty days against the first twenty of September, never against the whole of it
        assert_eq!(answer["compared_with"]["from"], json!("2026-09-01"));
        assert_eq!(answer["compared_with"]["to"], json!("2026-09-20"));
        assert_eq!(answer["compared_with"]["profit"], json!("0.000000"));
        assert_eq!(answer["change"]["profit"], json!("1000.000000"));
    }

    #[test]
    fn a_short_month_compares_only_the_days_it_has() {
        // February has no 31st, so a window closing on the 31st of March is compared with the 28 days February has
        let answer = read(&traded(), json!({"to": "2026-03-31"}));
        assert_eq!(answer["compared_with"]["from"], json!("2026-02-01"));
        assert_eq!(answer["compared_with"]["to"], json!("2026-02-28"));
    }

    #[test]
    fn a_window_that_opens_mid_month_is_compared_with_the_same_number_of_days_before_it() {
        let answer = read(&traded(), json!({"from": "2026-10-10", "to": "2026-10-20"}));
        // eleven days, against the eleven days that end the day before this window opens
        assert_eq!(answer["compared_with"]["from"], json!("2026-09-29"));
        assert_eq!(answer["compared_with"]["to"], json!("2026-10-09"));
        assert_eq!(answer["days"], json!(11));
    }

    #[test]
    fn what_is_owed_each_way_comes_from_the_documents_the_book_holds() {
        let answer = read(&traded(), json!({"to": "2026-10-31"}));
        assert_eq!(answer["owed_to_you"], json!("1150.000000"));
        assert_eq!(answer["owed_by_you"], json!("460.000000"));
    }

    #[test]
    fn the_tax_set_aside_is_the_tax_ledgers_own_closing_figure() {
        let answer = read(&traded(), json!({"to": "2026-10-31"}));
        assert_eq!(answer["tax"]["owed"], json!("90.000000"));
        assert!(answer["tax"]["accounts"].as_u64().unwrap_or(0) >= 1);
    }

    #[test]
    fn the_money_the_book_keeps_is_read_from_its_own_money_accounts() {
        let graph = traded();
        cashbook::create_account(
            &graph,
            &scope(),
            &json!({"account_code": "BANK-1", "account_name": "Trading account",
                    "account_type": "bank", "opening_balance": "500.00"}),
        )
        .expect("a money account");
        let answer = read(&graph, json!({"to": "2026-10-31"}));
        assert_eq!(answer["cash"]["total"], json!("500.000000"));
        assert_eq!(
            answer["cash"]["accounts"][0]["name"],
            json!("Trading account")
        );
    }

    #[test]
    fn a_book_with_no_money_accounts_states_nothing_held_rather_than_failing() {
        let answer = read(&traded(), json!({"to": "2026-10-31"}));
        assert_eq!(answer["cash"]["total"], json!("0.000000"));
        assert_eq!(
            answer["cash"]["accounts"]
                .as_array()
                .expect("accounts")
                .len(),
            0
        );
    }

    #[test]
    fn entries_that_are_not_posted_are_counted_and_the_profit_says_it_is_provisional() {
        let graph = traded();
        create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-10-12", "description": "still a note", "status": "draft",
                    "lines": [{"account_number": "1000", "debit": "300.00", "credit": "0"},
                              {"account_number": "4000", "debit": "0", "credit": "300.00"}]}),
        )
        .expect("a note");
        let answer = read(&graph, json!({"to": "2026-10-20"}));
        assert_eq!(answer["not_posted"]["drafts"], json!(1));
        assert_eq!(answer["not_posted"]["oldest"], json!("2026-10-12"));
        assert!(
            answer["note"]
                .as_str()
                .expect("a note")
                .contains("provisional"),
            "{}",
            answer["note"]
        );
        // the note is not in the figures: a draft is not a posting
        assert_eq!(answer["revenue"], json!("1150.000000"));
    }

    #[test]
    fn a_window_with_everything_posted_says_the_figures_are_the_ledgers_own() {
        let answer = read(&traded(), json!({"to": "2026-10-31"}));
        assert_eq!(answer["not_posted"]["drafts"], json!(0));
        assert_eq!(answer["not_posted"]["oldest"], Value::Null);
        assert!(answer["note"]
            .as_str()
            .expect("a note")
            .contains("the ledger's own"));
    }

    #[test]
    fn the_day_is_asked_for_rather_than_assumed() {
        let refused = progress(&traded(), &scope(), &json!({})).expect_err("no day");
        assert!(
            refused.to_string().contains("Say the day to report on"),
            "{refused}"
        );
    }

    #[test]
    fn a_day_that_is_not_a_day_is_refused_in_words() {
        let refused =
            progress(&traded(), &scope(), &json!({"to": "2026-02-30"})).expect_err("no such day");
        assert!(
            refused.to_string().contains("2026-02 has 28 days"),
            "{refused}"
        );
        let written_wrongly =
            progress(&traded(), &scope(), &json!({"to": "20 October"})).expect_err("not a day");
        assert!(
            written_wrongly.to_string().contains("2026-10-20"),
            "{written_wrongly}"
        );
    }

    #[test]
    fn a_window_that_closes_before_it_opens_is_refused() {
        let refused = progress(
            &traded(),
            &scope(),
            &json!({"from": "2026-10-20", "to": "2026-10-01"}),
        )
        .expect_err("backwards");
        assert!(refused.to_string().contains("before it opens"), "{refused}");
    }

    #[test]
    fn any_window_answers_over_its_own_days() {
        // a week inside the month: the sale on the 5th is in it, the purchase on the 9th is not
        let answer = read(&traded(), json!({"from": "2026-10-05", "to": "2026-10-08"}));
        assert_eq!(answer["revenue"], json!("1150.000000"));
        assert_eq!(answer["costs"], json!("150.000000"));
        assert_eq!(answer["profit"], json!("1000.000000"));
        // a quarter that contains both
        let quarter = read(&traded(), json!({"from": "2026-10-01", "to": "2026-12-31"}));
        assert_eq!(quarter["profit"], json!("1000.000000"));
        assert_eq!(quarter["days"], json!(92));
    }
}
