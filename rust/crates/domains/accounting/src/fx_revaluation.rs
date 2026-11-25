#![forbid(unsafe_code)]
//! Restating foreign balances at a period end, and posting the difference nobody has been paid yet.
//!
//! A debt of 1,000 dollars carried at 25.5 is 25,500 of the Book's money on the day it was raised. If the dollar is worth
//! 27 on the last day of the month, the business is owed 27,000 - it has gained 1,500 without anyone paying it anything.
//! That gain is real enough to report and not real enough to spend, which is exactly why it goes to its own account as an
//! unrealised difference rather than into sales.
//!
//! Two rules make the restatement safe to read afterwards. It is refused for a month that has been closed, because a
//! closed month is a figure somebody has already reported and cannot change. And it is one entry, dated on the last day of
//! the period, carrying a line per account restated - so what the rate did is visible, in the period it did it in, instead
//! of being smeared through the year.
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::{amount, wire};
use vimbai_store::read::{field, text};
use vimbai_store::{BookScope, Graph, StoreError};

use crate::ledger_book;
use crate::periods;

/// A balance held in another money, as it was posted and as much of that money as it represents.
#[derive(Debug, Clone, PartialEq)]
pub struct Balance {
    /// The account the balance sits in, in the Book's chart.
    pub account: String,
    /// How much of the foreign money is held or owed.
    pub units: Money,
    /// What that was carried at in the Book's money, at the rate it was posted at.
    pub carried: Money,
}

impl Balance {
    /// What this balance is worth at the closing rate.
    pub fn restated(&self, closing: &Money) -> Money {
        self.units.times(closing)
    }

    /// What the rate did to it: positive is a gain, negative a loss.
    pub fn difference(&self, closing: &Money) -> Money {
        vimbai_common::fx::difference(&self.carried, &self.restated(closing))
    }
}

/// What a restatement comes to, before anything is posted.
#[derive(Debug, Clone, Default)]
pub struct Restatement {
    /// One line per account: the account and the signed difference the rate made to it.
    pub accounts: Vec<(String, Money)>,
    /// Everything the rate added, as a positive figure.
    pub gain: Money,
    /// Everything the rate took away, as a positive figure.
    pub loss: Money,
}

impl Restatement {
    /// Whether the restatement changes anything at all.
    pub fn is_empty(&self) -> bool {
        self.gain.is_zero() && self.loss.is_zero()
    }
}

/// Works out what a closing rate does to a set of foreign balances, without posting anything.
///
/// Gains and losses are totalled separately rather than netted, because a Book that gained on one currency and lost on
/// another has done two things and should say so.
pub fn restatement(balances: &[Balance], closing: &Money) -> Restatement {
    let mut worked = Restatement::default();
    for balance in balances {
        let difference = balance.difference(closing);
        if difference.is_zero() {
            continue;
        }
        if difference.is_negative() {
            worked.loss = worked.loss.plus(&difference.abs());
        } else {
            worked.gain = worked.gain.plus(&difference);
        }
        worked.accounts.push((balance.account.clone(), difference));
    }
    worked
}

/// `POST /accounting/fx-revaluation`: restate foreign balances at a closing rate and post what the rate did.
///
/// [payload] carries `entry_date` (the last day of the period being restated), `closing_rate`, and `balances`, each with
/// `account`, `units` and `carried`. Nothing is posted for a month already closed: that month is a figure somebody has
/// reported, and a correction to it belongs in the current month.
pub fn revalue(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entry_date = text(payload, "entry_date");
    let period = periods::month_of(&entry_date).ok_or_else(|| {
        StoreError::BadRequest(format!(
            "{entry_date} is not a day. A restatement is dated on the last day of the period, written YYYY-MM-DD."
        ))
    })?;
    if periods::closed_months(graph, scope)?.contains(&period) {
        return Err(StoreError::BadRequest(format!(
            "{period} has been closed, so its figures cannot be restated. Post the restatement into the current month."
        )));
    }

    let closing = amount(payload, "closing_rate")?;
    if closing.is_zero() || closing.is_negative() {
        return Err(StoreError::BadRequest(
            "closing_rate must be more than nothing: it is what one unit of the foreign money is worth".into(),
        ));
    }

    let stated = payload
        .get("balances")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut balances: Vec<Balance> = Vec::with_capacity(stated.len());
    for held in &stated {
        balances.push(Balance {
            account: text(held, "account"),
            units: amount(held, "units")?,
            carried: amount(held, "carried")?,
        });
    }
    if balances.is_empty() {
        return Err(StoreError::BadRequest(
            "balances is empty: a restatement needs the foreign balances it is restating".into(),
        ));
    }

    let worked = restatement(&balances, &closing);
    if worked.is_empty() {
        return Ok(json!({
            "period": period,
            "closing_rate": wire(&closing),
            "exchange_gain": wire(&Money::zero()),
            "exchange_loss": wire(&Money::zero()),
            "restated": 0,
            "journal_entry_id": Value::Null,
            "note": "Nothing changed: every balance was already carried at the closing rate.",
        }));
    }

    let gain_account = crate::subsidiary_journals::account_for_purpose(
        graph,
        scope,
        &text(payload, "exchange_gain_account"),
        "exchange_gain",
        "revenue",
    )?;
    let loss_account = crate::subsidiary_journals::account_for_purpose(
        graph,
        scope,
        &text(payload, "exchange_loss_account"),
        "exchange_loss",
        "expense",
    )?;
    let zero = Money::zero();
    let mut lines: Vec<Value> = Vec::new();
    for (account, difference) in &worked.accounts {
        if difference.is_negative() {
            let lost = difference.abs();
            lines.push(
                json!({"account_number": account, "debit": wire(&zero), "credit": wire(&lost),
                              "description": format!("Restated at the closing rate for {period}")}),
            );
            lines.push(
                json!({"account_number": loss_account, "debit": wire(&lost), "credit": wire(&zero),
                              "description": format!("Loss on the rate at {period}")}),
            );
        } else {
            lines.push(
                json!({"account_number": account, "debit": wire(difference), "credit": wire(&zero),
                              "description": format!("Restated at the closing rate for {period}")}),
            );
            lines.push(json!({"account_number": gain_account, "debit": wire(&zero), "credit": wire(difference),
                              "description": format!("Gain on the rate at {period}")}));
        }
    }

    let entry = json!({
        "entry_date": entry_date,
        "description": format!("Restatement of foreign balances at {period}"),
        "reference_number": format!("FX-{period}"),
        "source_module": "ExchangeRevaluation",
        "status": "posted",
        "created_at": text(payload, "created_at"),
        "lines": lines,
    });
    let posted = ledger_book::create_entry(graph, scope, &entry)?;

    Ok(json!({
        "period": period,
        "closing_rate": wire(&closing),
        "exchange_gain": wire(&worked.gain),
        "exchange_loss": wire(&worked.loss),
        "restated": worked.accounts.len(),
        "accounts": worked.accounts.iter().map(|(account, difference)| json!({
            "account_number": account,
            "difference": wire(difference),
        })).collect::<Vec<Value>>(),
        "journal_entry_id": field(&posted, "id").to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn money(text: &str) -> Money {
        Money::parse(text).expect("a figure")
    }

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_debt_in_a_stronger_money_gains_without_anyone_paying() {
        let balances = vec![Balance {
            account: "1100".into(),
            units: money("1000"),
            carried: money("25500"),
        }];
        let worked = restatement(&balances, &money("27"));
        assert_eq!(
            worked.gain.to_fixed(2),
            "1500.00",
            "1,000 at 27 is 27,000 against 25,500 carried"
        );
        assert!(worked.loss.is_zero());
        assert_eq!(worked.accounts, vec![("1100".to_string(), money("1500"))]);
    }

    #[test]
    fn a_gain_and_a_loss_are_totalled_apart_rather_than_netted() {
        let balances = vec![
            Balance {
                account: "1100".into(),
                units: money("1000"),
                carried: money("25500"),
            },
            Balance {
                account: "2000".into(),
                units: money("-1000"),
                carried: money("-25500"),
            },
        ];
        let worked = restatement(&balances, &money("27"));
        assert_eq!(worked.gain.to_fixed(2), "1500.00");
        assert_eq!(
            worked.loss.to_fixed(2),
            "1500.00",
            "two currencies, two things happened"
        );
    }

    #[test]
    fn a_balance_already_at_the_closing_rate_is_left_alone() {
        let balances = vec![Balance {
            account: "1100".into(),
            units: money("1000"),
            carried: money("27000"),
        }];
        assert!(restatement(&balances, &money("27")).is_empty());
    }

    #[test]
    fn a_closed_month_cannot_be_restated() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "p-1", "period": "2026-10"})]]);
        let refusal = revalue(
            &graph,
            &scope(),
            &json!({
                "entry_date": "2026-10-31",
                "closing_rate": "27",
                "balances": [{"account": "1100", "units": "1000", "carried": "25500"}],
            }),
        );
        let message = match refusal {
            Ok(_) => String::new(),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("2026-10 has been closed"), "{message}");
    }

    #[test]
    fn a_restatement_needs_a_day_a_rate_and_some_balances() {
        let graph = FakeGraph::queue(vec![vec![]]);
        let no_day = revalue(
            &graph,
            &scope(),
            &json!({"closing_rate": "27", "balances": []}),
        );
        assert!(no_day.is_err(), "a restatement is dated");
        let graph = FakeGraph::queue(vec![vec![]]);
        let no_rate = revalue(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-10-31", "balances": []}),
        );
        assert!(no_rate.is_err(), "a restatement is at a rate");
    }
}
