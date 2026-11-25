#![forbid(unsafe_code)]
//! Settling a document raised in another money, and posting the difference the rate made.
//!
//! A sale in dollars is posted at the rate of the day it was raised. When the customer pays, the money arrives at the
//! rate of the day it was paid, and the two figures are not the same. That difference is real: it is why a business can
//! sell at a profit and still have less money than it expected. It is not sales income and it is not a cost of trading,
//! so it is posted to its own account rather than being left inside receivables or quietly folded into revenue.
//!
//! The arithmetic is `vimbai_common::fx`, so the ledger, a report and the phone all agree on what a difference is. What
//! this module adds is the posting: the money in, the debt cleared, and the difference placed on the side it belongs.
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::{amount, wire};
use vimbai_store::read::text;

use crate::subsidiary_journals::account_for_purpose;
use vimbai_store::{BookScope, Graph, StoreError};

use crate::ledger_book;

/// Which side of the books a foreign document sits on: what a customer owes the business, or what the business owes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSide {
    /// A sale: the business is owed, and the money arriving clears a receivable.
    Receivable,
    /// A bill: the business owes, and the money leaving clears a payable.
    Payable,
}

impl DocumentSide {
    /// The side a caller named, or the receivable side when it named none, which is the ordinary case for a receipt.
    pub fn named(payload: &Value) -> Self {
        match text(payload, "document_side")
            .trim()
            .to_lowercase()
            .as_str()
        {
            "payable" | "bill" | "purchase" => Self::Payable,
            _ => Self::Receivable,
        }
    }
}

/// The accounts a settlement posts to, and where the difference goes.
#[derive(Debug, Clone)]
pub struct SettlementAccounts {
    /// Where the money actually arrived: a bank or cash account.
    pub bank: String,
    /// The debt being cleared, carried at the rate the document was raised at.
    pub receivable: String,
    /// Where a gain on the rate goes: revenue, because the business is better off.
    pub gain: String,
    /// Where a loss on the rate goes: an expense, because the business is worse off.
    pub loss: String,
}

/// What the rate did to a settlement: a gain, or a loss, and never both.
///
/// The two figures are returned separately rather than as one signed number, so a caller cannot post a negative credit
/// and hope the ledger reads it as a debit.
pub fn settled(carried: &Money, received: &Money) -> (Money, Money) {
    settled_on(DocumentSide::Receivable, carried, received)
}

/// What the rate did to a settlement, on whichever side of the books it happened.
///
/// The same movement of the rate is good news on one side and bad on the other: a stronger foreign money means a customer
/// pays more than was carried (a gain) and a supplier must be paid more than was carried (a loss). Working that out from
/// the side rather than from a sign the caller chose is what stops a payable being reported as income.
pub fn settled_on(side: DocumentSide, carried: &Money, received: &Money) -> (Money, Money) {
    let difference = vimbai_common::fx::difference(carried, received);
    let gain = match side {
        DocumentSide::Receivable => difference,
        DocumentSide::Payable => difference.negate(),
    };
    if gain.is_negative() {
        (Money::zero(), gain.abs())
    } else {
        (gain, Money::zero())
    }
}

/// Posts the settlement of a foreign document: money in, the debt cleared, and the rate's difference placed.
///
/// [payload] carries `entry_date`, `reference_number`, `carried` (what the document was posted at, in the Book's money),
/// `received` (what the payment is worth in the Book's money at the day it was paid) and optionally the four accounts.
/// A settlement at exactly the carried figure posts no difference line at all, because nothing happened.
pub fn settle_foreign(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let carried = amount(payload, "carried")?;
    let received = amount(payload, "received")?;
    if carried.is_zero() || carried.is_negative() {
        return Err(StoreError::BadRequest(
            "carried must be more than nothing: it is what the document was posted at".into(),
        ));
    }
    if received.is_zero() || received.is_negative() {
        return Err(StoreError::BadRequest(
            "received must be more than nothing: it is what the payment is worth".into(),
        ));
    }

    // The Book's own accounts, asked for by purpose. A number written here would post a business's money to whatever
    // account happened to carry that number, or to nothing at all, and the entry would still balance.
    let accounts = SettlementAccounts {
        bank: account_for_purpose(
            graph,
            scope,
            &text(payload, "bank_account"),
            "bank",
            "asset",
        )?,
        receivable: match DocumentSide::named(payload) {
            DocumentSide::Receivable => account_for_purpose(
                graph,
                scope,
                &text(payload, "receivable_account"),
                "receivable",
                "asset",
            )?,
            DocumentSide::Payable => account_for_purpose(
                graph,
                scope,
                &text(payload, "payable_account"),
                "payable",
                "liability",
            )?,
        },
        gain: account_for_purpose(
            graph,
            scope,
            &text(payload, "exchange_gain_account"),
            "exchange_gain",
            "revenue",
        )?,
        loss: account_for_purpose(
            graph,
            scope,
            &text(payload, "exchange_loss_account"),
            "exchange_loss",
            "expense",
        )?,
    };
    let side = DocumentSide::named(payload);
    let (gain, loss) = settled_on(side, &carried, &received);
    let zero = Money::zero();
    let reference = text(payload, "reference_number");

    let mut lines = match side {
        // a sale: the money arrives, and the debt the customer owed is cleared
        DocumentSide::Receivable => vec![
            json!({"account_number": accounts.bank, "debit": wire(&received), "credit": wire(&zero),
                   "description": "Money received"}),
            json!({"account_number": accounts.receivable, "debit": wire(&zero), "credit": wire(&carried),
                   "description": "Debt cleared at the rate it was raised at"}),
        ],
        // a bill: the debt the business owed is cleared, and the money leaves
        DocumentSide::Payable => vec![
            json!({"account_number": accounts.receivable, "debit": wire(&carried), "credit": wire(&zero),
                   "description": "Debt cleared at the rate it was raised at"}),
            json!({"account_number": accounts.bank, "debit": wire(&zero), "credit": wire(&received),
                   "description": "Money paid"}),
        ],
    };
    if !gain.is_zero() {
        lines.push(
            json!({"account_number": accounts.gain, "debit": wire(&zero), "credit": wire(&gain),
                          "description": "Gain on the rate when the debt was paid"}),
        );
    }
    if !loss.is_zero() {
        lines.push(
            json!({"account_number": accounts.loss, "debit": wire(&loss), "credit": wire(&zero),
                          "description": "Loss on the rate when the debt was paid"}),
        );
    }

    let entry = json!({
        "entry_date": text(payload, "entry_date"),
        "description": format!("Settlement of {reference}"),
        "reference_number": reference,
        "source_module": "ExchangeSettlement",
        "status": "posted",
        "created_at": text(payload, "created_at"),
        "lines": lines,
    });
    let posted = ledger_book::create_entry(graph, scope, &entry)?;

    Ok(json!({
        "reference_number": reference,
        "document_side": match side { DocumentSide::Receivable => "receivable", DocumentSide::Payable => "payable" },
        "carried": wire(&carried),
        "received": wire(&received),
        "exchange_gain": wire(&gain),
        "exchange_loss": wire(&loss),
        "journal_entry_id": posted["id"].clone(),
    }))
}

/// `POST /accounting/fx-difference/`: the difference the rate made, posted on its own.
///
/// A payment is recorded once, by whatever recorded the money moving: the cash book owns that entry, and it clears the debt
/// at the amount that left. When the debt was carried at an earlier rate, that leaves the document account standing at the
/// wrong figure, and the difference between the two is what this posts. Nothing about the money is recorded twice: only the
/// side the rate moved, against the account it moved on and the Book's own exchange account.
pub fn settle_difference<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let (carried, received) = settlement_figures(payload)?;
    if carried.is_zero() || received.is_zero() {
        return Err(StoreError::BadRequest(
            "carried and received must both be more than nothing: they are the two rates the difference is between"
                .into(),
        ));
    }
    let side = DocumentSide::named(payload);
    let (gain, loss) = settled_on(side, &carried, &received);
    if gain.is_zero() && loss.is_zero() {
        return Err(StoreError::BadRequest(
            "The rate made no difference on this settlement, so there is nothing to post.".into(),
        ));
    }

    // the same rule as a settlement: the Book's own accounts, asked for by purpose rather than assumed by number
    let accounts = SettlementAccounts {
        bank: account_for_purpose(
            graph,
            scope,
            &text(payload, "bank_account"),
            "bank",
            "asset",
        )?,
        receivable: match side {
            DocumentSide::Receivable => account_for_purpose(
                graph,
                scope,
                &text(payload, "receivable_account"),
                "receivable",
                "asset",
            )?,
            DocumentSide::Payable => account_for_purpose(
                graph,
                scope,
                &text(payload, "payable_account"),
                "payable",
                "liability",
            )?,
        },
        gain: account_for_purpose(
            graph,
            scope,
            &text(payload, "exchange_gain_account"),
            "exchange_gain",
            "revenue",
        )?,
        loss: account_for_purpose(
            graph,
            scope,
            &text(payload, "exchange_loss_account"),
            "exchange_loss",
            "expense",
        )?,
    };
    let zero = Money::zero();
    let reference = text(payload, "reference_number");
    let _ = &accounts.bank;

    // the document account is standing at the wrong figure by exactly the difference, and it is put right on the side that
    // moves it back: a gain on a debt owed to the business leaves receivables short, a loss on a debt it owes leaves payables
    // over-cleared
    let mut lines = Vec::new();
    match (side, gain.is_zero()) {
        (DocumentSide::Receivable, false) => {
            lines.push(json!({"account_number": accounts.receivable, "debit": wire(&gain), "credit": wire(&zero),
                              "description": "Debt put right for the rate it was paid at"}));
            lines.push(
                json!({"account_number": accounts.gain, "debit": wire(&zero), "credit": wire(&gain),
                              "description": "Gain on the rate when the debt was paid"}),
            );
        }
        (DocumentSide::Receivable, true) => {
            lines.push(json!({"account_number": accounts.receivable, "debit": wire(&zero), "credit": wire(&loss),
                              "description": "Debt put right for the rate it was paid at"}));
            lines.push(
                json!({"account_number": accounts.loss, "debit": wire(&loss), "credit": wire(&zero),
                              "description": "Loss on the rate when the debt was paid"}),
            );
        }
        (DocumentSide::Payable, false) => {
            lines.push(json!({"account_number": accounts.receivable, "debit": wire(&gain), "credit": wire(&zero),
                              "description": "Debt put right for the rate it was paid at"}));
            lines.push(
                json!({"account_number": accounts.gain, "debit": wire(&zero), "credit": wire(&gain),
                              "description": "Gain on the rate when the debt was paid"}),
            );
        }
        (DocumentSide::Payable, true) => {
            lines.push(json!({"account_number": accounts.receivable, "debit": wire(&zero), "credit": wire(&loss),
                              "description": "Debt put right for the rate it was paid at"}));
            lines.push(
                json!({"account_number": accounts.loss, "debit": wire(&loss), "credit": wire(&zero),
                              "description": "Loss on the rate when the debt was paid"}),
            );
        }
    }

    let entry = json!({
        "entry_date": text(payload, "entry_date"),
        "description": format!("Rate difference on {reference}"),
        "reference_number": reference,
        "source_module": "ExchangeSettlement",
        "status": "posted",
        "created_at": text(payload, "created_at"),
        "lines": lines,
    });
    let posted = ledger_book::create_entry(graph, scope, &entry)?;
    Ok(json!({
        "reference_number": reference,
        "document_side": match side { DocumentSide::Receivable => "receivable", DocumentSide::Payable => "payable" },
        "exchange_gain": wire(&gain),
        "exchange_loss": wire(&loss),
        "journal_entry_id": posted["id"].clone(),
    }))
}

/// What a settlement cleared, worked out from what the document said when the caller is paying part of it.
///
/// A part payment of a foreign bill clears a share of the debt, not all of it, and the share has to be exact: the same
/// largest-remainder split the rest of the ledger uses, so a bill paid in three parts carries exactly what it said rather
/// than a few thousandths over or under. A caller that already knows both figures hands them over as they are.
fn settlement_figures(payload: &Value) -> Result<(Money, Money), StoreError> {
    let paying = payload.get("units_paid").filter(|value| !value.is_null());
    let Some(units_paid) = paying.map(|_| amount(payload, "units_paid")).transpose()? else {
        return Ok((amount(payload, "carried")?, amount(payload, "received")?));
    };
    let units_total = amount(payload, "units_total")?;
    let carried_total = amount(payload, "carried_total")?;
    let rate = amount(payload, "rate")?;
    if units_total.is_zero() || units_total.is_negative() {
        return Err(StoreError::BadRequest(
            "units_total must be more than nothing: it is what the document was billed for".into(),
        ));
    }
    if units_paid.is_zero() || units_paid.is_negative() || units_paid > units_total {
        return Err(StoreError::BadRequest(
            "units_paid must be more than nothing and no more than the document was billed for"
                .into(),
        ));
    }
    if rate.is_zero() || rate.is_negative() {
        return Err(StoreError::BadRequest(
            "rate must be more than nothing: it is what the money was worth when it was paid"
                .into(),
        ));
    }
    // the debt cleared is this payment's share of what was carried, to the ledger's own unit
    let left = units_total.minus(&units_paid);
    let share = carried_total.allocate(&[units_paid.clone(), left], 2);
    let carried = share.first().cloned().unwrap_or_else(Money::zero);
    Ok((carried, units_paid.times(&rate)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn money(text: &str) -> Money {
        Money::parse(text).expect("a figure")
    }

    #[test]
    fn a_rate_that_moved_in_our_favour_is_a_gain_and_nothing_else() {
        let (gain, loss) = settled(&money("36000"), &money("38000"));
        assert_eq!(gain.to_fixed(2), "2000.00");
        assert!(loss.is_zero(), "a gain is never also a loss");
    }

    #[test]
    fn a_rate_that_moved_against_us_is_a_loss_with_its_sign_placed() {
        let (gain, loss) = settled(&money("36000"), &money("34000"));
        assert!(gain.is_zero());
        assert_eq!(
            loss.to_fixed(2),
            "2000.00",
            "the loss is a positive figure on its own side"
        );
    }

    #[test]
    fn a_settlement_at_the_carried_figure_leaves_no_difference() {
        let (gain, loss) = settled(&money("36000"), &money("36000"));
        assert!(gain.is_zero() && loss.is_zero());
    }

    #[test]
    fn part_of_a_bill_clears_its_share_of_what_was_carried() {
        // 1,000 USD carried at 25.5 is 25,500; paying 400 of it clears 10,200 and costs 10,800 at 27
        let payload = json!({
            "units_paid": "400",
            "units_total": "1000",
            "carried_total": "25500",
            "rate": "27",
        });
        let (carried, received) = settlement_figures(&payload).expect("a part payment reads");
        assert_eq!(carried.to_fixed(2), "10200.00");
        assert_eq!(received.to_fixed(2), "10800.00");
    }

    #[test]
    fn a_part_payment_never_clears_more_than_the_document_carried() {
        // a third of a bill whose carried figure does not divide evenly still adds back to exactly what was carried
        let payload = json!({
            "units_paid": "333.33",
            "units_total": "1000",
            "carried_total": "25500",
            "rate": "26",
        });
        let (first, _) = settlement_figures(&payload).expect("a part payment reads");
        let rest = json!({
            "units_paid": "666.67",
            "units_total": "1000",
            "carried_total": "25500",
            "rate": "26",
        });
        let (second, _) = settlement_figures(&rest).expect("the rest reads");
        assert_eq!(
            first.plus(&second).to_fixed(2),
            "25500.00",
            "the two parts together are the debt, not a penny more or less"
        );
    }

    #[test]
    fn paying_more_than_the_document_was_billed_for_is_refused() {
        let payload = json!({
            "units_paid": "1001",
            "units_total": "1000",
            "carried_total": "25500",
            "rate": "27",
        });
        let refused = settlement_figures(&payload);
        let message = match refused {
            Ok(_) => String::new(),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains("no more than the document was billed for"),
            "{message}"
        );
    }

    #[test]
    fn a_settlement_that_the_rate_did_not_move_is_refused_rather_than_posted_as_nothing() {
        let graph = FakeGraph::queue(vec![]);
        let payload = json!({
            "entry_date": "2026-11-20",
            "reference_number": "RC-1",
            "carried": "25500",
            "received": "25500",
            "document_side": "payable",
        });
        let refused = settle_difference(&graph, &BookScope::new("user-1", "book-1"), &payload);
        let message = match refused {
            Ok(_) => String::new(),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("made no difference"), "{message}");
    }

    #[test]
    fn the_same_rate_movement_is_a_gain_on_one_side_and_a_loss_on_the_other() {
        // 25,500 carried, 27,000 paid: a customer paying more is income, a supplier being paid more is a cost
        let (gain, loss) = settled_on(DocumentSide::Receivable, &money("25500"), &money("27000"));
        assert_eq!(gain.to_fixed(2), "1500.00");
        assert!(loss.is_zero());
        let (gain, loss) = settled_on(DocumentSide::Payable, &money("25500"), &money("27000"));
        assert!(
            gain.is_zero(),
            "paying more than was carried never reads as income"
        );
        assert_eq!(loss.to_fixed(2), "1500.00");
    }

    #[test]
    fn a_cheaper_settlement_gains_on_the_payable_side() {
        let (gain, loss) = settled_on(DocumentSide::Payable, &money("25500"), &money("24000"));
        assert_eq!(gain.to_fixed(2), "1500.00");
        assert!(loss.is_zero());
    }
}
