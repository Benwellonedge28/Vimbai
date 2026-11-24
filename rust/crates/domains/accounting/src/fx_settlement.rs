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
use vimbai_store::read::{or_default, text};
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

impl SettlementAccounts {
    /// The accounts a business Book opens with, unless the caller names its own.
    pub fn standard() -> Self {
        Self {
            bank: "1010".to_string(),
            receivable: "1100".to_string(),
            gain: "4800".to_string(),
            loss: "5800".to_string(),
        }
    }
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

    let accounts = SettlementAccounts {
        bank: or_default(payload, "bank_account", "1010"),
        receivable: match DocumentSide::named(payload) {
            DocumentSide::Receivable => or_default(payload, "receivable_account", "1100"),
            DocumentSide::Payable => or_default(payload, "payable_account", "2000"),
        },
        gain: or_default(payload, "exchange_gain_account", "4800"),
        loss: or_default(payload, "exchange_loss_account", "5800"),
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_standard_book_knows_where_the_rate_difference_goes() {
        let accounts = SettlementAccounts::standard();
        assert_eq!(accounts.gain, "4800");
        assert_eq!(accounts.loss, "5800");
    }
}
