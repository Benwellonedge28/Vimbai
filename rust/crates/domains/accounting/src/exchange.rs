#![forbid(unsafe_code)]
//! What the rate did to the books, read from what the ledger has actually posted.
//!
//! A business that trades in more than one money makes and loses money on the rate itself, and those figures are already in
//! the ledger: a gain sits as a credit in the Book's exchange gain account, a loss as a debit in its exchange loss account.
//! Nothing here multiplies a balance by a rate, because a rate this file chose would be a claim about somebody's position
//! that the books do not support.
//!
//! Two things are told apart, because they are not the same news. A difference on something already settled is money in
//! hand, and a difference on a balance still held is a figure that could move again before it is ever paid. A reader who
//! cannot see which is which will spend an unrealised gain.
use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money_or_nothing, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, JOURNAL_ENTRY, JOURNAL_LINE};

use crate::ledger_book;

/// Which side of the rate a Book's account holds: what it gained, or what it lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateSide {
    /// A gain: revenue, credited when the rate moved in the Book's favour.
    Gain,
    /// A loss: an expense, debited when the rate moved against it.
    Loss,
}

/// Which side an account is, if it is an account the rate posts to at all.
///
/// The name says so, and the type decides the side, because a Book may number its accounts however it likes: an exchange
/// gain is revenue and an exchange loss is an expense, and no number is assumed.
pub fn rate_side(account: &Value) -> Option<RateSide> {
    let name = field(account, "name").to_lowercase();
    if !name.contains("exchange") && !name.contains("rate") {
        return None;
    }
    match field(account, "account_type") {
        "revenue" => Some(RateSide::Gain),
        "expense" => Some(RateSide::Loss),
        _ => None,
    }
}

/// Whether an entry's difference has been settled, or is a restatement of something still held.
fn realised(entry: &Value) -> bool {
    field(entry, "source_module") == "ExchangeSettlement"
}

/// `GET /accounting/exchange-differences/`: what the rate did over a period, settled and unsettled told apart.
pub fn exchange_differences<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let from = text(query, "from");
    let until = text(query, "to");
    if from.is_empty() || until.is_empty() {
        return Err(refused(
            "Say the period to report on: from and to, as dates like 2026-10-01 and 2026-10-31.",
        ));
    }
    if from > until {
        return Err(refused(&format!(
            "The period runs backwards: from {from} is after to {until}."
        )));
    }

    let chart = ledger_book::accounts(graph, scope)?;
    let sides: Vec<(String, String, RateSide)> = chart
        .iter()
        .filter_map(|account| {
            let side = rate_side(account)?;
            let number = field(account, "account_number").to_string();
            if number.is_empty() {
                return None;
            }
            Some((number, field(account, "name").to_string(), side))
        })
        .collect();

    let mut settled_gain = Money::zero();
    let mut settled_loss = Money::zero();
    let mut held_gain = Money::zero();
    let mut held_loss = Money::zero();
    let mut per_account: Vec<Value> = Vec::new();
    let mut entries: Vec<Value> = Vec::new();

    for entry in Records::new(graph, JOURNAL_ENTRY).list_with_children(scope, 4000, JOURNAL_LINE)? {
        if field(&entry, "status") != "posted" {
            continue;
        }
        let date = field(&entry, "entry_date").to_string();
        if date.as_str() < from.as_str() || date.as_str() > until.as_str() {
            continue;
        }
        let money_in_hand = realised(&entry);
        let mut entry_gain = Money::zero();
        let mut entry_loss = Money::zero();
        for line in entry
            .get("lines")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let number = field(line, "account_number");
            let Some((_, _, side)) = sides.iter().find(|(held, _, _)| held == number) else {
                continue;
            };
            let debit = money_or_nothing(line, "debit")?;
            let credit = money_or_nothing(line, "credit")?;
            match side {
                // a gain is credited, a loss debited; anything on the other side of those accounts is a correction
                RateSide::Gain => {
                    let gained = credit.minus(&debit);
                    if gained.is_negative() {
                        entry_loss = entry_loss.plus(&gained.abs());
                    } else {
                        entry_gain = entry_gain.plus(&gained);
                    }
                }
                RateSide::Loss => {
                    let lost = debit.minus(&credit);
                    if lost.is_negative() {
                        entry_gain = entry_gain.plus(&lost.abs());
                    } else {
                        entry_loss = entry_loss.plus(&lost);
                    }
                }
            }
        }
        if entry_gain.is_zero() && entry_loss.is_zero() {
            continue;
        }
        if money_in_hand {
            settled_gain = settled_gain.plus(&entry_gain);
            settled_loss = settled_loss.plus(&entry_loss);
        } else {
            held_gain = held_gain.plus(&entry_gain);
            held_loss = held_loss.plus(&entry_loss);
        }
        entries.push(json!({
            "entry_date": date,
            "reference_number": field(&entry, "reference_number"),
            "description": field(&entry, "description"),
            "settled": money_in_hand,
            "gain": wire(&entry_gain),
            "loss": wire(&entry_loss),
        }));
    }

    for (number, name, side) in &sides {
        per_account.push(json!({
            "account_number": number,
            "name": name,
            "side": match side { RateSide::Gain => "gain", RateSide::Loss => "loss" },
        }));
    }

    let gain = settled_gain.plus(&held_gain);
    let loss = settled_loss.plus(&held_loss);
    Ok(json!({
        "from": from,
        "to": until,
        "gain": wire(&gain),
        "loss": wire(&loss),
        "net": wire(&gain.minus(&loss)),
        "settled": {"gain": wire(&settled_gain), "loss": wire(&settled_loss),
                    "net": wire(&settled_gain.minus(&settled_loss))},
        "held": {"gain": wire(&held_gain), "loss": wire(&held_loss),
                 "net": wire(&held_gain.minus(&held_loss))},
        "accounts": per_account,
        "entries": entries,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_gain_is_revenue_and_a_loss_is_an_expense_whatever_the_number() {
        let gain =
            json!({"account_number": "9900", "name": "Exchange Gain", "account_type": "revenue"});
        assert_eq!(rate_side(&gain), Some(RateSide::Gain));
        let loss =
            json!({"account_number": "1111", "name": "Exchange Loss", "account_type": "expense"});
        assert_eq!(rate_side(&loss), Some(RateSide::Loss));
        // a receivable is not a place the rate posts, however it is named
        let receivable = json!({"account_number": "1100", "name": "Accounts Receivable", "account_type": "asset"});
        assert_eq!(rate_side(&receivable), None);
    }

    #[test]
    fn the_period_is_required_and_cannot_run_backwards() {
        let graph = FakeGraph::queue(vec![]);
        let no_period = exchange_differences(&graph, &scope(), &json!({}));
        assert!(no_period.is_err(), "a report is over a period");
        let graph = FakeGraph::queue(vec![]);
        let backwards = exchange_differences(
            &graph,
            &scope(),
            &json!({"from": "2026-11-30", "to": "2026-11-01"}),
        );
        let message = match backwards {
            Ok(_) => String::new(),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("runs backwards"), "{message}");
    }
}
