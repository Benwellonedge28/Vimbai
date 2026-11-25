#![forbid(unsafe_code)]
//! What the Book owes the taxman, read from the tax it has actually posted.
//!
//! The figure comes from the ledger rather than from a rate: the tax on each document is posted to the Book's own tax
//! account when the document is posted, so what was collected is the credit side of that account over the period and what
//! can be reclaimed is the debit side. Nothing here multiplies a total by a percentage, because a percentage this file
//! chose would be a claim about somebody's tax position that the books do not support.
//!
//! The documents behind the figure are listed with it, so a person can follow a total to the invoice it came from.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money_or_nothing, refused, text};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, JOURNAL_ENTRY, JOURNAL_LINE, PURCHASES_JOURNAL,
    SALES_JOURNAL,
};

use crate::ledger_book;

/// Whether an account is one the tax is *owed* in, rather than the expense a sale is charged: the name says so, and the
/// expense account a sale charges its tax to is not a place tax is owed, so counting it would double the figure.
pub(crate) fn holds_tax(account: &Value) -> bool {
    let name = field(account, "name").to_lowercase();
    let expense = field(account, "account_type") == "expense";
    !expense && (name.contains("vat") || name.contains("tax"))
}

/// Every account in this Book's chart that tax is owed in, as its number and what the chart calls it.
///
/// The summary and the ledger both read tax from these accounts, so they are found in one place: two answers about which
/// accounts hold tax would eventually disagree, and a liability would be reported twice or not at all.
pub(crate) fn tax_accounts<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<Vec<(String, String)>, StoreError> {
    let chart = ledger_book::accounts(graph, scope)?;
    Ok(chart
        .iter()
        .filter(|account| holds_tax(account))
        .map(|account| {
            (
                field(account, "account_number").to_string(),
                field(account, "name").to_string(),
            )
        })
        .filter(|(number, _)| !number.is_empty())
        .collect())
}

/// `GET /accounting/tax/`: what was collected, what can be reclaimed, and what is owed, over a period.
pub fn tax_due<G: Graph + ?Sized>(
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

    let tax_accounts = tax_accounts(graph, scope)?;

    let mut collected = Money::zero();
    let mut reclaimable = Money::zero();
    let mut per_account: Vec<Value> = Vec::new();
    let mut debits: std::collections::BTreeMap<String, Money> = Default::default();
    let mut credits: std::collections::BTreeMap<String, Money> = Default::default();

    // the lines are read once and placed against the account they moved, so the figure is the ledger's own
    for entry in Records::new(graph, JOURNAL_ENTRY).list_with_children(scope, 4000, JOURNAL_LINE)? {
        if field(&entry, "status") != "posted" {
            continue;
        }
        let date = field(&entry, "entry_date");
        if date < from.as_str() || date > until.as_str() {
            continue;
        }
        for line in entry
            .get("lines")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let number = field(line, "account_number");
            if !tax_accounts.iter().any(|(held, _)| held == number) {
                continue;
            }
            let debit = money_or_nothing(line, "debit")?;
            let credit = money_or_nothing(line, "credit")?;
            if !debit.is_zero() {
                let slot = debits.entry(number.to_string()).or_insert_with(Money::zero);
                *slot = slot.plus(&debit);
                reclaimable = reclaimable.plus(&debit);
            }
            if !credit.is_zero() {
                let slot = credits
                    .entry(number.to_string())
                    .or_insert_with(Money::zero);
                *slot = slot.plus(&credit);
                collected = collected.plus(&credit);
            }
        }
    }

    for (number, name) in &tax_accounts {
        let taken = credits.get(number).cloned().unwrap_or_else(Money::zero);
        let paid = debits.get(number).cloned().unwrap_or_else(Money::zero);
        per_account.push(json!({
            "account_number": number,
            "name": name,
            "collected": wire(&taken),
            "reclaimable": wire(&paid),
            "due": wire(&taken.minus(&paid)),
        }));
    }

    // the documents that put the tax there, so a total can be followed to the invoice behind it
    let mut documents: Vec<Value> = Vec::new();
    for (kind, kind_of_record, number_key, date_key) in [
        ("sale", SALES_JOURNAL, "invoice_number", "invoice_date"),
        (
            "purchase",
            PURCHASES_JOURNAL,
            "purchase_order_number",
            "purchase_date",
        ),
    ] {
        for document in Records::new(graph, kind_of_record).list(scope, 4000)? {
            let tax = money_or_nothing(&document, "tax_amount")?;
            if tax.is_zero() {
                continue;
            }
            let date = field(&document, date_key);
            if date < from.as_str() || date > until.as_str() {
                continue;
            }
            documents.push(json!({
                "kind": kind,
                "number": field(&document, number_key),
                "date": date,
                "total": field(&document, "total_amount"),
                "tax": wire(&tax),
            }));
        }
    }
    documents.sort_by(|one, two| field(two, "date").cmp(field(one, "date")));

    Ok(json!({
        "from": from,
        "to": until,
        "collected": wire(&collected),
        "reclaimable": wire(&reclaimable),
        "due": wire(&collected.minus(&reclaimable)),
        "accounts": per_account,
        "documents": documents,
        // said plainly, because a Book whose chart holds no tax account cannot have posted any tax at all
        "has_tax_account": !tax_accounts.is_empty(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::seed_opening_chart;
    use crate::subsidiary_journals::{create_purchase, create_sale};
    use serde_json::json;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A business Book with its opening chart, which holds a VAT account, and one taxed sale and one taxed purchase.
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

    fn report(graph: &MemoryGraph, from: &str, to: &str) -> Value {
        tax_due(graph, &scope(), &json!({"from": from, "to": to})).expect("the reading")
    }

    #[test]
    fn what_was_collected_and_what_can_be_reclaimed_come_from_the_ledger() {
        let answer = report(&traded(), "2026-10-01", "2026-10-31");
        assert_eq!(answer["collected"], json!("150.000000"));
        assert_eq!(answer["reclaimable"], json!("60.000000"));
        assert_eq!(answer["due"], json!("90.000000"));
        assert_eq!(answer["has_tax_account"], json!(true));
        assert_eq!(answer["accounts"][0]["account_number"], json!("2100"));
        assert_eq!(answer["accounts"][0]["due"], json!("90.000000"));
    }

    #[test]
    fn the_documents_behind_the_figure_are_listed_with_it() {
        let answer = report(&traded(), "2026-10-01", "2026-10-31");
        let documents = answer["documents"].as_array().expect("documents");
        assert_eq!(documents.len(), 2);
        // newest first, so the purchase on the 9th comes before the sale on the 5th
        assert_eq!(documents[0]["kind"], json!("purchase"));
        assert_eq!(documents[0]["number"], json!("PO-1"));
        assert_eq!(documents[0]["tax"], json!("60.000000"));
        assert_eq!(documents[1]["kind"], json!("sale"));
        assert_eq!(documents[1]["number"], json!("INV-1"));
        assert_eq!(documents[1]["tax"], json!("150.000000"));
    }

    #[test]
    fn a_period_with_no_trade_owes_nothing_and_says_nothing_was_collected() {
        let answer = report(&traded(), "2026-11-01", "2026-11-30");
        assert_eq!(answer["collected"], json!("0.000000"));
        assert_eq!(answer["due"], json!("0.000000"));
        assert_eq!(answer["documents"], json!([]));
    }

    #[test]
    fn a_book_whose_chart_holds_no_tax_account_says_so_rather_than_reporting_a_nil_return() {
        // a Book that never seeded its chart has nothing to post tax to, so there is nothing to report
        let graph = MemoryGraph::new();
        let answer = report(&graph, "2026-10-01", "2026-10-31");
        assert_eq!(answer["has_tax_account"], json!(false));
        assert_eq!(answer["accounts"], json!([]));
        assert_eq!(answer["due"], json!("0.000000"));
    }

    #[test]
    fn a_period_that_is_not_two_dates_or_runs_backwards_is_refused() {
        let graph = traded();
        let missing =
            tax_due(&graph, &scope(), &json!({"from": "2026-10-01"})).expect_err("no end");
        assert!(missing.to_string().contains("from and to"), "{missing}");
        let backwards = tax_due(
            &graph,
            &scope(),
            &json!({"from": "2026-10-31", "to": "2026-10-01"}),
        )
        .expect_err("backwards");
        assert!(
            backwards.to_string().contains("runs backwards"),
            "{backwards}"
        );
    }

    #[test]
    fn a_reclaim_larger_than_what_was_collected_answers_as_money_coming_back() {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        create_purchase(
            &graph,
            &scope(),
            &json!({"purchase_order_number": "PO-9", "entry_date": "2026-10-09",
                    "total_amount": "2300.00", "tax_amount": "300.00"}),
        )
        .expect("a taxed purchase");
        let answer = report(&graph, "2026-10-01", "2026-10-31");
        assert_eq!(answer["collected"], json!("0.000000"));
        assert_eq!(answer["due"], json!("-300.000000"));
    }
}
