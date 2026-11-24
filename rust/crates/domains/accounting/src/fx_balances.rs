#![forbid(unsafe_code)]
//! The documents a Book holds in another money, listed by currency, so a restatement can start from the books.
//!
//! A period end restatement needs to know what foreign balances exist and what they were carried at. That information is
//! already in the books: a sale or a bill raised in another money recorded its own currency, its own total and the rate it
//! was carried at, and kept them beside the Book's figures rather than instead of them.
//!
//! What is still owed is worked out by subtraction, and only where the books recorded enough to subtract honestly. A receipt
//! against a foreign document keeps the money it was in, how much of that money arrived and which document it was against,
//! so a settlement reduces the document it names. What is left of the Book's figure is allocated in proportion to what is
//! left of the foreign money, which is the ordinary simplification and is stated in the answer rather than left for a
//! reader to assume.
//!
//! A receipt that names no document is reported as unmatched instead of being spread across the others: money that arrived
//! without saying what it was for has not told anybody which debt it cleared.
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money_or_nothing, text};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, CASH_DISBURSEMENTS_JOURNAL, CASH_ENTRY,
    CASH_RECEIPTS_JOURNAL, SALES_JOURNAL, VENDOR_BILL,
};

/// One document raised in another money, as the books recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignDocument {
    /// What the document is called: an invoice number or a bill number.
    pub reference: String,
    /// Whether the Book raised it (a sale) or received it (a bill).
    pub kind: &'static str,
    /// The day it was raised.
    pub day: String,
    /// The money it was raised in.
    pub currency: String,
    /// The rate it was carried at.
    pub rate: Money,
    /// What it said, in its own money.
    pub units: Money,
    /// What it was carried at, in the Book's money.
    pub carried: Money,
}

/// Money that arrived against a document raised in another money.
#[derive(Debug, Clone, PartialEq)]
pub struct Settled {
    /// The money it arrived in.
    pub currency: String,
    /// The document it was against, as the books named it.
    pub document: String,
    /// How much of that money arrived.
    pub units: Money,
}

/// What is left of a document after the settlements recorded against it.
#[derive(Debug, Clone, PartialEq)]
pub struct Outstanding {
    /// How much of the foreign money is still owed.
    pub units: Money,
    /// What that is carried at in the Book's money, allocated in proportion to what is left.
    pub carried: Money,
}

/// What is left of [document] once [settled] of its money has arrived.
///
/// The Book's figure is reduced in proportion to the foreign money that is left, because a settlement clears part of a debt
/// and not a slice of a rate. A document settled in full leaves nothing, whatever the arithmetic of rounding would say.
pub fn outstanding_after(document: &ForeignDocument, settled: &Money) -> Outstanding {
    if settled.is_zero() || settled.is_negative() {
        return Outstanding {
            units: document.units.clone(),
            carried: document.carried.clone(),
        };
    }
    if settled >= &document.units {
        return Outstanding {
            units: Money::zero(),
            carried: Money::zero(),
        };
    }
    let left = document.units.minus(settled);
    let carried = if document.units.is_zero() {
        Money::zero()
    } else {
        document.carried.times(&left).over(&document.units).round(2)
    };
    Outstanding {
        units: left,
        carried,
    }
}

/// What one currency's documents come to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CurrencyTotal {
    /// The money these documents are in.
    pub currency: String,
    /// The documents themselves, in date order.
    pub documents: Vec<ForeignDocument>,
    /// Everything they said, in that money.
    pub units: Money,
    /// Everything they were carried at, in the Book's money.
    pub carried: Money,
}

/// Groups documents by the money they are in, and totals each group.
///
/// Currencies are kept apart rather than converted to one figure, because a Book holding dollars and rand has two
/// questions to answer at a period end and adding them at some rate of this file's choosing would answer neither.
pub fn by_currency(documents: Vec<ForeignDocument>) -> Vec<CurrencyTotal> {
    let mut totals: Vec<CurrencyTotal> = Vec::new();
    for document in documents {
        let at = match totals
            .iter()
            .position(|total| total.currency == document.currency)
        {
            Some(at) => at,
            None => {
                totals.push(CurrencyTotal {
                    currency: document.currency.clone(),
                    ..CurrencyTotal::default()
                });
                totals.len() - 1
            }
        };
        let total = &mut totals[at];
        total.units = total.units.plus(&document.units);
        total.carried = total.carried.plus(&document.carried);
        total.documents.push(document);
    }
    for total in &mut totals {
        total
            .documents
            .sort_by(|left, right| left.day.cmp(&right.day));
    }
    totals.sort_by(|left, right| left.currency.cmp(&right.currency));
    totals
}

/// The documents of one journal that were raised in another money.
fn abroad(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: &'static str,
    journal: vimbai_store::RecordKind,
    reference_field: &str,
) -> Result<Vec<ForeignDocument>, StoreError> {
    let mut documents: Vec<ForeignDocument> = Vec::new();
    for record in Records::new(graph, journal).list(scope, 4000)? {
        let currency = text(&record, "document_currency").trim().to_uppercase();
        if currency.is_empty() {
            continue;
        }
        let rate = money_or_nothing(&record, "document_rate")?;
        if rate.is_zero() {
            // a document in another money with no rate is not a document this can restate; it is refused by the posting
            // path that should have caught it, so it is skipped here rather than shown at a rate of nothing
            continue;
        }
        documents.push(ForeignDocument {
            reference: field(&record, reference_field).to_string(),
            kind,
            day: field(&record, "invoice_date").to_string(),
            currency,
            rate,
            units: money_or_nothing(&record, "document_total")?,
            carried: money_or_nothing(&record, "total_amount")?,
        });
    }
    Ok(documents)
}

/// How much of [document]'s foreign money has already arrived, as the cash journals recorded it.
///
/// A caller that needs to know what a single document still owes asks this rather than reading the journals itself: the rule
/// for what counts as a settlement lives in one place, so a listing and a balance can never disagree about it.
pub fn settled_for<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    currency: &str,
    document: &str,
) -> Result<Money, StoreError> {
    let currency = currency.trim().to_uppercase();
    let mut cleared = Money::zero();
    for settlement in settlements(graph, scope)? {
        if settlement.currency != currency || settlement.document != document {
            continue;
        }
        cleared = cleared.plus(&settlement.units);
    }
    Ok(cleared)
}

/// The settlements the receipts journal recorded against foreign documents.
fn settlements(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Settled>, StoreError> {
    // money coming in clears what a customer owed; money going out clears what the business owes, and a bill paid at a
    // different rate has been settled just as a sale paid at a different rate has
    let mut settled: Vec<Settled> = Vec::new();
    for journal in [CASH_RECEIPTS_JOURNAL, CASH_DISBURSEMENTS_JOURNAL] {
        settled.extend(recorded_against(graph, scope, journal)?);
    }
    settled.extend(cash_book_settlements(graph, scope)?);
    Ok(settled)
}

/// The settlements the cash book recorded, which keeps the money it was in and the rate it was recorded at.
///
/// A cash book entry already says what money it was in, how much of that money moved and which voucher it was against, so a
/// payment recorded there clears a foreign bill as surely as one recorded in a journal does. An entry in the Book's own
/// money is not a foreign settlement and is skipped, and so is one with no voucher, because a movement that names no
/// document has not said which debt it cleared.
fn cash_book_settlements(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Settled>, StoreError> {
    let book = crate::document_currency::book_currency(graph, scope)
        .trim()
        .to_uppercase();
    let mut settled: Vec<Settled> = Vec::new();
    for record in Records::new(graph, CASH_ENTRY).list(scope, 4000)? {
        let currency = text(&record, "currency").trim().to_uppercase();
        if currency.is_empty() || currency == book {
            continue;
        }
        let document = text(&record, "voucher_number").trim().to_string();
        if document.is_empty() {
            continue;
        }
        let units = money_or_nothing(&record, "amount")?;
        if units.is_zero() {
            continue;
        }
        settled.push(Settled {
            currency,
            document,
            units,
        });
    }
    Ok(settled)
}

/// The settlements one cash journal recorded against foreign documents.
fn recorded_against(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    journal: vimbai_store::RecordKind,
) -> Result<Vec<Settled>, StoreError> {
    let mut settled: Vec<Settled> = Vec::new();
    for record in Records::new(graph, journal).list(scope, 4000)? {
        let currency = text(&record, "document_currency").trim().to_uppercase();
        if currency.is_empty() {
            continue;
        }
        let units = money_or_nothing(&record, "document_units")?;
        if units.is_zero() {
            // the money arrived without saying how much of the foreign money it cleared, so it is not counted here
            continue;
        }
        settled.push(Settled {
            currency,
            document: field(&record, "document_reference").to_string(),
            units,
        });
    }
    Ok(settled)
}

/// `GET /accounting/fx-balances/`: the documents this Book holds in another money, by currency.
pub fn fx_balances<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let mut documents = abroad(graph, scope, "sale", SALES_JOURNAL, "invoice_number")?;
    documents.extend(abroad(graph, scope, "bill", VENDOR_BILL, "bill_number")?);
    let settled = settlements(graph, scope)?;

    // what each document still owes, and which settlements named nothing this Book can find
    let mut left: Vec<(ForeignDocument, Outstanding)> = Vec::with_capacity(documents.len());
    let mut unmatched: Vec<Value> = Vec::new();
    for document in documents {
        let mut cleared = Money::zero();
        for settlement in &settled {
            if settlement.currency != document.currency {
                continue;
            }
            if settlement.document.is_empty() || settlement.document != document.reference {
                continue;
            }
            cleared = cleared.plus(&settlement.units);
        }
        let outstanding = outstanding_after(&document, &cleared);
        left.push((document, outstanding));
    }
    for settlement in &settled {
        let found = left.iter().any(|(document, _)| {
            document.reference == settlement.document && document.currency == settlement.currency
        });
        if !found {
            unmatched.push(json!({
                "currency": settlement.currency,
                "document": settlement.document,
                "units": wire(&settlement.units),
            }));
        }
    }
    let totals = by_currency(left.iter().map(|(document, _)| document.clone()).collect());

    Ok(json!({
        "currencies": totals.iter().map(|total| {
            let rows: Vec<Value> = total.documents.iter().map(|document| {
                let outstanding = left.iter()
                    .find(|(held, _)| held.reference == document.reference && held.currency == document.currency)
                    .map(|(_, outstanding)| outstanding.clone())
                    .unwrap_or(Outstanding { units: document.units.clone(), carried: document.carried.clone() });
                json!({
                    "reference": document.reference,
                    "kind": document.kind,
                    "date": document.day,
                    "rate": wire(&document.rate),
                    "units": wire(&document.units),
                    "carried": wire(&document.carried),
                    "outstanding_units": wire(&outstanding.units),
                    "outstanding_carried": wire(&outstanding.carried),
                })
            }).collect();
            let units = rows.iter().fold(Money::zero(), |sum, row| {
                sum.plus(&money_or_nothing(row, "outstanding_units").unwrap_or_else(|_| Money::zero()))
            });
            let carried = rows.iter().fold(Money::zero(), |sum, row| {
                sum.plus(&money_or_nothing(row, "outstanding_carried").unwrap_or_else(|_| Money::zero()))
            });
            json!({
                "currency": total.currency,
                "units": wire(&units),
                "carried": wire(&carried),
                "documents": rows,
            })
        }).collect::<Vec<Value>>(),
        "unmatched_settlements": unmatched,
        "note": "What is still owed, worked out by subtraction where a receipt named the document it cleared. What is left \
                 of the Book's figure is allocated in proportion to what is left of the foreign money. A receipt that named \
                 no document is listed as unmatched rather than spread across the others.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn money(text: &str) -> Money {
        Money::parse(text).expect("a figure")
    }

    fn document(currency: &str, day: &str, units: &str, carried: &str) -> ForeignDocument {
        ForeignDocument {
            reference: "DOC-1".into(),
            kind: "sale",
            day: day.into(),
            currency: currency.into(),
            rate: money("25.5"),
            units: money(units),
            carried: money(carried),
        }
    }

    #[test]
    fn each_money_is_totalled_on_its_own_rather_than_converted_together() {
        let totals = by_currency(vec![
            document("USD", "2026-11-02", "1000", "25500"),
            document("USD", "2026-11-01", "500", "12750"),
            document("ZAR", "2026-11-03", "2000", "2800"),
        ]);
        assert_eq!(totals.len(), 2, "two monies, two answers");
        let usd = totals
            .iter()
            .find(|t| t.currency == "USD")
            .expect("dollars");
        assert_eq!(usd.units.to_fixed(2), "1500.00");
        assert_eq!(usd.carried.to_fixed(2), "38250.00");
        assert_eq!(usd.documents.first().expect("a document").day, "2026-11-01");
        let zar = totals.iter().find(|t| t.currency == "ZAR").expect("rand");
        assert_eq!(zar.units.to_fixed(2), "2000.00");
    }

    #[test]
    fn a_part_payment_leaves_the_rest_of_the_foreign_money_and_its_carried_figure() {
        let document = document("USD", "2026-11-02", "1000", "25500");
        let left = outstanding_after(&document, &money("400"));
        assert_eq!(left.units.to_fixed(2), "600.00");
        // 25,500 carried against 1,000 dollars: 600 of them are carried at 15,300
        assert_eq!(left.carried.to_fixed(2), "15300.00");
    }

    #[test]
    fn a_document_settled_in_full_owes_nothing_whatever_the_rounding_says() {
        let document = document("USD", "2026-11-02", "1000", "25500");
        let left = outstanding_after(&document, &money("1000"));
        assert!(left.units.is_zero() && left.carried.is_zero());
        let overpaid = outstanding_after(&document, &money("1200"));
        assert!(
            overpaid.units.is_zero(),
            "paying more than was owed does not leave a negative debt"
        );
    }

    #[test]
    fn nothing_settled_leaves_the_document_exactly_as_it_was() {
        let document = document("USD", "2026-11-02", "1000", "25500");
        let left = outstanding_after(&document, &Money::zero());
        assert_eq!(left.units.to_fixed(2), "1000.00");
        assert_eq!(left.carried.to_fixed(2), "25500.00");
    }

    #[test]
    fn a_book_with_nothing_abroad_says_nothing_rather_than_inventing_a_currency() {
        let graph = FakeGraph::queue(vec![vec![], vec![], vec![]]);
        let listed = fx_balances(&graph, &BookScope::new("user-1", "book-1")).expect("listed");
        assert_eq!(listed["currencies"].as_array().map(Vec::len), Some(0));
    }
}
