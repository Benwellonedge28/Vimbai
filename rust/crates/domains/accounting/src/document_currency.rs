#![forbid(unsafe_code)]
//! A document raised in another currency, posted into the money the Book keeps its own books in.
//!
//! A Book keeps one set of books, in one money. A customer may still be invoiced in another: a Zimbabwean Book invoices
//! a customer in US dollars. What must never happen is the dollar figure landing in a ledger kept in the Book's money,
//! because then a trial balance adds dollars to bond notes and the total means nothing.
//!
//! So a document that names another currency must also state the rate it was raised at, and its figures are carried
//! into the Book's money before anything is posted. The document keeps what it actually said (its currency, its rate,
//! its own total) so the register can show the customer's money while the journal shows the Book's.
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{money, money_or_nothing, refused, text};
use vimbai_store::{BookScope, Graph, StoreError};

/// What a foreign document said, kept so the register can show it as the customer saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentMoney {
    /// The money the document was raised in.
    pub currency: String,
    /// The rate it was raised at: one unit of the document's money is this much of the Book's.
    pub rate: Money,
    /// The total the customer was given, in the document's own money.
    pub total: Money,
    /// Any discount taken off that total, in the document's own money.
    pub discount: Money,
    /// Any tax carried out of that total, in the document's own money.
    pub tax: Money,
}

/// The money this Book keeps its own books in.
pub fn book_currency(graph: &(impl Graph + ?Sized), scope: &BookScope) -> String {
    crate::standards::reporting_currency(graph, scope)
}

/// Rewrites a document's figures into the Book's money, and says what it carried.
///
/// A document already in the Book's money is handed back untouched, so nothing changes for the ordinary case. One in
/// another currency must name its rate: the refusal says so and names the field, rather than posting a figure whose
/// money nobody can tell.
pub fn in_book_money(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<(Value, Option<DocumentMoney>), StoreError> {
    // a document that says nothing about money is already in the Book's own, and asking the Book costs a read
    let stated_currency = vimbai_common::document_money::named_currency(payload.get("currency"));
    if stated_currency.is_empty() && payload.get("exchange_rate").is_none() {
        return Ok((payload.clone(), None));
    }
    let book = book_currency(graph, scope);
    let currency = if stated_currency.is_empty() {
        book.clone()
    } else {
        stated_currency
    };

    if currency == book {
        // a rate against itself is not a rate; saying one is a mistake worth naming
        if let Ok(stated) = money(payload, "exchange_rate") {
            if !stated.is_zero() && stated != Money::parse("1").unwrap_or_else(|_| Money::zero()) {
                return Err(refused(&format!(
                    "This document is in {book}, which is the money this Book keeps its books in, so the exchange \
                     rate must be 1 or left out; it was {}.",
                    wire(&stated)
                )));
            }
        }
        return Ok((payload.clone(), None));
    }

    let rate = match vimbai_common::document_money::exchange_rate(&currency, payload.get("exchange_rate")) {
        Ok(Some(rate)) => rate,
        Ok(None) => {
            return Err(refused(&format!(
                "This document is in {currency} but this Book keeps its books in {book}, so the rate is needed: give \
                 exchange_rate as how much one {currency} is worth in {book}, and the journal will post {book}."
            )))
        }
        Err(why) => return Err(refused(&why)),
    };

    let total = money(payload, "total_amount")?;
    let discount = money_or_nothing(payload, "discount_amount")?;
    let tax = money_or_nothing(payload, "tax_amount")?;

    // one rounding per figure, in the Book's money, so the lines still add up to the total that is posted
    let in_book = |amount: &Money| amount.times(&rate).round(2);
    let (book_total, book_discount, book_tax) =
        (in_book(&total), in_book(&discount), in_book(&tax));

    let mut carried = payload.clone();
    if let Some(object) = carried.as_object_mut() {
        object.insert("currency".into(), json!(book));
        object.insert("total_amount".into(), json!(wire(&book_total)));
        if !discount.is_zero() {
            object.insert("discount_amount".into(), json!(wire(&book_discount)));
        }
        if !tax.is_zero() {
            object.insert("tax_amount".into(), json!(wire(&book_tax)));
        }
        // the money the customer's own document was in, kept beside the Book's figures
        object.insert("document_currency".into(), json!(currency));
        object.insert("document_rate".into(), json!(wire(&rate)));
        object.insert("document_total".into(), json!(wire(&total)));
        if !discount.is_zero() {
            object.insert("document_discount".into(), json!(wire(&discount)));
        }
        if !tax.is_zero() {
            object.insert("document_tax".into(), json!(wire(&tax)));
        }
    }
    let said = text(payload, "invoice_number");
    let _ = said;

    Ok((
        carried,
        Some(DocumentMoney {
            currency,
            rate,
            total,
            discount,
            tax,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_store::fake::MemoryGraph;

    fn book_in(currency: &str) -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = BookScope::new("user-1", "book-1");
        vimbai_store::Records::new(&graph, vimbai_store::STANDARDS_CONFIG)
            .create(
                &scope,
                &json!({"organization_id": "org-1", "reporting_currency": currency}),
            )
            .expect("the Book states its money");
        (graph, scope)
    }

    #[test]
    fn a_dollar_document_in_a_bond_book_is_carried_into_bond_money_before_it_is_posted() {
        let (graph, scope) = book_in("ZWL");
        let (carried, said) = in_book_money(
            &graph,
            &scope,
            &json!({"currency": "USD", "total_amount": "1000.00", "exchange_rate": "25.5",
                    "discount_amount": "0"}),
        )
        .expect("a stated rate carries the document");
        // 1,000 dollars at 25.5 is 25,500 bond notes, and that is the figure the journal will hold
        assert_eq!(
            money(&carried, "total_amount").expect("the Book's figure"),
            Money::parse("25500.00").expect("25,500 bond notes")
        );
        assert_eq!(text(&carried, "currency"), "ZWL");
        let said = said.expect("the document was foreign");
        assert_eq!(said.currency, "USD");
        assert_eq!(
            said.total,
            Money::parse("1000.00").expect("the customer's figure")
        );
        // the customer's own figure is kept beside it, so the register shows what they were given
        assert_eq!(
            money(&carried, "document_total").expect("the customer's figure"),
            Money::parse("1000.00").expect("a thousand dollars")
        );
        assert_eq!(text(&carried, "document_currency"), "USD");
    }

    #[test]
    fn a_document_already_in_the_books_money_is_untouched() {
        let (graph, scope) = book_in("ZWL");
        let payload = json!({"currency": "ZWL", "total_amount": "250.00"});
        let (carried, said) = in_book_money(&graph, &scope, &payload).expect("nothing to convert");
        assert_eq!(carried, payload);
        assert!(said.is_none());
    }

    #[test]
    fn a_foreign_document_without_a_rate_is_refused_and_the_field_is_named() {
        let (graph, scope) = book_in("ZWL");
        let refused_one = in_book_money(
            &graph,
            &scope,
            &json!({"currency": "USD", "total_amount": "1000.00"}),
        )
        .expect_err("a dollar figure cannot be posted into a bond book unstated");
        let said = format!("{refused_one}");
        assert!(said.contains("exchange_rate"), "{said}");
        assert!(said.contains("ZWL"), "{said}");
    }

    #[test]
    fn a_rate_of_nothing_or_less_is_refused() {
        let (graph, scope) = book_in("ZWL");
        for rate in ["0", "-25.5"] {
            let refused_one = in_book_money(
                &graph,
                &scope,
                &json!({"currency": "USD", "total_amount": "1000.00", "exchange_rate": rate}),
            )
            .expect_err("a rate must be more than nothing");
            assert!(
                format!("{refused_one}").contains("more than nothing"),
                "{rate}"
            );
        }
    }

    #[test]
    fn a_document_in_the_books_own_money_cannot_claim_another_rate() {
        let (graph, scope) = book_in("ZWL");
        let refused_one = in_book_money(
            &graph,
            &scope,
            &json!({"currency": "ZWL", "total_amount": "250.00", "exchange_rate": "25.5"}),
        )
        .expect_err("bond notes do not convert to themselves at 25.5");
        assert!(
            format!("{refused_one}").contains("must be 1"),
            "{refused_one}"
        );
    }
}
