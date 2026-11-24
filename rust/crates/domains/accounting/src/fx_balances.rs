#![forbid(unsafe_code)]
//! The documents a Book holds in another money, listed by currency, so a restatement can start from the books.
//!
//! A period end restatement needs to know what foreign balances exist and what they were carried at. That information is
//! already in the books: a sale or a bill raised in another money recorded its own currency, its own total and the rate it
//! was carried at, and kept them beside the Book's figures rather than instead of them.
//!
//! What this route does not claim is that a document listed here is still outstanding. A settlement records the money that
//! arrived and the difference the rate made, but not how much of the foreign money it cleared, so nothing here can subtract
//! one from the other honestly. The listing says so in its own answer rather than presenting a document as a balance and
//! letting somebody restate a debt that has already been paid.
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money_or_nothing, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, SALES_JOURNAL, VENDOR_BILL};

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

/// `GET /accounting/fx-balances/`: the documents this Book holds in another money, by currency.
pub fn fx_balances<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let mut documents = abroad(graph, scope, "sale", SALES_JOURNAL, "invoice_number")?;
    documents.extend(abroad(graph, scope, "bill", VENDOR_BILL, "bill_number")?);
    let totals = by_currency(documents);

    Ok(json!({
        "currencies": totals.iter().map(|total| json!({
            "currency": total.currency,
            "units": wire(&total.units),
            "carried": wire(&total.carried),
            "documents": total.documents.iter().map(|document| json!({
                "reference": document.reference,
                "kind": document.kind,
                "date": document.day,
                "rate": wire(&document.rate),
                "units": wire(&document.units),
                "carried": wire(&document.carried),
            })).collect::<Vec<Value>>(),
        })).collect::<Vec<Value>>(),
        "note": "What the books recorded, not what is still owed: a settlement does not record how much of the foreign \
                 money it cleared, so a document here may already be partly paid. Restate the balances still held.",
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
    fn a_book_with_nothing_abroad_says_nothing_rather_than_inventing_a_currency() {
        let graph = FakeGraph::queue(vec![vec![], vec![]]);
        let listed = fx_balances(&graph, &BookScope::new("user-1", "book-1")).expect("listed");
        assert_eq!(listed["currencies"].as_array().map(Vec::len), Some(0));
    }
}
