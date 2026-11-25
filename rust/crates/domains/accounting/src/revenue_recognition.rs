#![forbid(unsafe_code)]
//! revenue-recognition-service, on the store.
//!
//! A contract with a customer is a set of performance obligations, each satisfied its own way. The
//! transaction price is split across them by relative standalone selling price, revenue is recognised
//! obligation by obligation as each is satisfied, and whatever is not yet recognised stays deferred.
//!
//! The arithmetic, all exact:
//!
//! * the contract's transaction price is the sum of what the caller priced each obligation at,
//! * allocation only runs when at least one obligation carries a standalone selling price: each
//!   obligation that has one is re-priced at its share of the total, and each that does not keeps the
//!   price it was given,
//! * recognised revenue on an obligation is capped at that obligation's price, so it cannot run past
//!   what was sold,
//! * deferred revenue is the contract price less everything recognised so far.
//!
//! Kept as shipped, and it is the reason allocation is documented rather than tightened: an obligation
//! with no standalone selling price keeps its own figure while the others are re-priced from the whole,
//! so the allocated parts need not add back to the total. That is what the service did, and a contract
//! already allocated that way would change underneath its own record if this were corrected.
//!
//! Also as shipped: a recognition call with no amount recognises the whole obligation remaining, not
//! zero. It means "recognise it in full", which is how the endpoint has always been used.
//!
//! What the books say: recognising revenue posts what was earned now as Dr Accounts Receivable, Cr Sales, so the
//! contract record and the ledger cannot disagree about how much has been earned. A contract on its own posts nothing,
//! because an agreement to sell is not a sale. Both accounts are found by type and name in the Book's own chart, and a
//! Book that has no receivable account is told what is missing rather than posting into the wrong one.

use serde_json::{json, Value};
use vimbai_common::Money;

use crate::subsidiary_journals::{account_named, must_be_open, post};
use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::read::{refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, REVENUE_CONTRACT};

fn obligations(payload: &Value) -> Vec<Value> {
    payload
        .get("obligations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// One obligation, with the figures the allocation and recognition steps work on.
#[derive(Clone, Debug)]
pub struct Obligation {
    pub description: String,
    pub price: Money,
    pub standalone: Money,
    pub method: String,
    pub satisfied: bool,
    pub recognised: Money,
}

impl Obligation {
    fn read(row: &Value) -> Result<Self, StoreError> {
        Ok(Self {
            description: text(row, "description"),
            price: amount_or_zero(row, "transaction_price")?,
            standalone: amount_or_zero(row, "standalone_selling_price")?,
            method: if text(row, "recognition_method").is_empty() {
                "point_in_time".to_string()
            } else {
                text(row, "recognition_method")
            },
            satisfied: row
                .get("is_satisfied")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            recognised: amount_or_zero(row, "revenue_recognized")?,
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "description": self.description,
            "transaction_price": wire(&self.price),
            "standalone_selling_price": wire(&self.standalone),
            "recognition_method": self.method,
            "is_satisfied": self.satisfied,
            "revenue_recognized": wire(&self.recognised),
        })
    }
}

/// Split the transaction price by relative standalone selling price, where one exists.
///
/// An obligation with no standalone price keeps the price it was given. That is why the parts need not
/// add back to the total, and why this is a documented shape rather than a bug to fix.
pub fn allocate_price(total: &Money, rows: &mut [Obligation]) {
    let standalone_total = rows
        .iter()
        .fold(Money::zero(), |sum, row| sum.plus(&row.standalone));
    if standalone_total <= Money::zero() || *total <= Money::zero() {
        return;
    }
    for row in rows.iter_mut() {
        if row.standalone > Money::zero() {
            let share = row.standalone.over(&standalone_total);
            row.price = total.times(&share);
        }
    }
}

/// Recognise against one obligation, capped at what it is worth.
pub fn recognise(row: &mut Obligation, asked: &Money) -> Money {
    let amount = if *asked > Money::zero() {
        asked.clone()
    } else {
        row.price.clone()
    };
    let raised = row.recognised.plus(&amount);
    row.recognised = if raised > row.price {
        row.price.clone()
    } else {
        raised
    };
    row.satisfied = row.recognised >= row.price;
    row.recognised.clone()
}

fn read_rows(contract: &Value) -> Result<Vec<Obligation>, StoreError> {
    let rows = contract
        .get("obligations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.iter().map(Obligation::read).collect()
}

fn total_of(rows: &[Obligation]) -> Money {
    rows.iter()
        .fold(Money::zero(), |sum, row| sum.plus(&row.price))
}

fn recognised_of(rows: &[Obligation]) -> Money {
    rows.iter()
        .fold(Money::zero(), |sum, row| sum.plus(&row.recognised))
}

/// `/contracts`.
pub fn create_contract<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let mut rows: Vec<Obligation> = obligations(payload)
        .iter()
        .map(Obligation::read)
        .collect::<Result<_, _>>()?;

    // the contract price is what the caller priced the parts at, before any allocation
    let total = total_of(&rows);
    if rows.iter().any(|row| row.standalone > Money::zero()) {
        allocate_price(&total, &mut rows);
    }
    let recognised = recognised_of(&rows);
    let deferred = total.minus(&recognised);

    Records::new(graph, REVENUE_CONTRACT).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "customer_name": text(payload, "customer_name"),
            "contract_date": text(payload, "contract_date"),
            "total_transaction_price": wire(&total),
            "total_revenue_recognized": wire(&recognised),
            "deferred_revenue": wire(&deferred),
            "status": "active",
            "obligations": rows.iter().map(Obligation::to_json).collect::<Vec<_>>(),
        }),
    )
}

/// `/contracts/{id}/recognize`: recognise against one obligation, write the contract back, and post what was earned.
///
/// The contract record says what is deferred; the books have to say the same thing, or a reader comparing the two finds
/// revenue that exists in one place and not the other. So what this call recognises is posted as the customer owing it
/// and the business having earned it: Dr Accounts Receivable, Cr Sales.
///
/// Only the part recognised now is posted, not the whole obligation: recognising an obligation in instalments posts an
/// entry per instalment, and a call that recognises nothing new posts nothing at all rather than an entry for zero. A
/// contract on its own moves no books, because an agreement to sell is not a sale; the money moves when the business has
/// done what it promised.
///
/// The day is `entry_date` from the call, or the contract's own date when the call does not say. A month that has been
/// closed is refused in words rather than written into.
///
/// The reference carries the contract, the obligation and the figure now recognised, so a repeat of the same
/// recognition is refused by the ledger instead of counting the same revenue twice, while a further instalment is a
/// different figure and posts as its own entry.
pub fn recognise_revenue<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    contract_id: &str,
    obligation_id: &str,
    asked: &Money,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, REVENUE_CONTRACT);
    let contract = records.get(scope, contract_id)?;
    let mut rows = read_rows(&contract)?;

    // obligations are addressed by their place in the list, as the store keeps no separate id on them
    let index = obligation_id
        .parse::<usize>()
        .map_err(|_| StoreError::NotFound)?;
    let (before, recognised, satisfied, what) = {
        let row = rows.get_mut(index).ok_or(StoreError::NotFound)?;
        let before = row.recognised.clone();
        let recognised = recognise(row, asked);
        let what = if row.description.trim().is_empty() {
            format!("obligation {obligation_id}")
        } else {
            row.description.clone()
        };
        (before, recognised, row.satisfied, what)
    };
    let earned_now = recognised.minus(&before);

    let total = stored_amount(&contract, "total_transaction_price")?;
    let recognised_total = recognised_of(&rows);
    let deferred = total.minus(&recognised_total);

    // what was earned now is what the books move, and the entry is posted before the contract says it was: a contract
    // that claims revenue the ledger never took is the worse of the two ways to fail
    let mut entry_id = Value::Null;
    if earned_now > Money::zero() {
        let stated = text(payload, "entry_date");
        let day = if stated.trim().is_empty() {
            text(&contract, "contract_date")
        } else {
            stated
        };
        if !reads_as_a_day(&day) {
            return Err(refused(
                "Say the day this revenue was earned, as 2026-11-20, in entry_date.",
            ));
        }
        must_be_open(graph, scope, &day)?;
        let receivable = account_named(
            graph,
            scope,
            payload,
            "receivable_account",
            "receivable",
            "asset",
        )?;
        let revenue = account_named(graph, scope, payload, "revenue_account", "sales", "revenue")?;
        if receivable == revenue {
            return Err(refused(
                "What the customer owes and what the business earned cannot be the same account.",
            ));
        }
        let customer = text(&contract, "customer_name");
        let description = if customer.trim().is_empty() {
            format!("Revenue recognised: {what}")
        } else {
            format!("Revenue recognised: {customer}, {what}")
        };
        // the digits the ledger stores, not the JSON string that wraps them
        let reference = format!(
            "REV-{contract_id}-{obligation_id}-{}",
            recognised.to_fixed(6)
        );
        let dated = json!({"entry_date": day, "created_at": text(payload, "created_at")});
        entry_id = json!(post(
            graph,
            scope,
            &dated,
            description,
            &reference,
            REVENUE_MODULE,
            &receivable,
            &revenue,
            &earned_now,
            &format!("Revenue earned on {what}"),
        )?);
    }

    let mut written = json!({
        "obligations": rows.iter().map(Obligation::to_json).collect::<Vec<_>>(),
        "total_revenue_recognized": wire(&recognised_total),
        "deferred_revenue": wire(&deferred),
    });
    if !entry_id.is_null() {
        written["last_journal_entry_id"] = entry_id.clone();
    }
    records.update(scope, contract_id, &written)?;

    Ok(json!({
        "obligation_id": obligation_id,
        "recognized": wire(&recognised),
        "recognized_now": wire(&earned_now),
        "is_satisfied": satisfied,
        "contract_total_recognized": wire(&recognised_total),
        "deferred": wire(&deferred),
        "journal_entry_id": entry_id,
    }))
}

/// The source module recognised revenue posts under, which the ledger keeps references unique for.
const REVENUE_MODULE: &str = "RevenueRecognition";

/// Whether a stated day reads as one, so a posting cannot be dated by a word.
fn reads_as_a_day(day: &str) -> bool {
    crate::periods::month_of(day).is_some()
}

/// `/contracts/{company_id}`.
pub fn list_contracts<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, REVENUE_CONTRACT).list(scope, 1000)?;
    let mine: Vec<Value> = rows
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    Ok(json!({
        "company_id": company_id,
        "contracts": mine,
        "total": mine.len(),
    }))
}

/// `/summary/{company_id}`.
pub fn revenue_summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, REVENUE_CONTRACT).list(scope, 1000)?;
    let mine: Vec<Value> = rows
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    let mut value = Money::zero();
    let mut recognised = Money::zero();
    let mut deferred = Money::zero();
    for row in &mine {
        value = value.plus(&stored_amount(row, "total_transaction_price")?);
        recognised = recognised.plus(&stored_amount(row, "total_revenue_recognized")?);
        deferred = deferred.plus(&stored_amount(row, "deferred_revenue")?);
    }
    Ok(json!({
        "company_id": company_id,
        "total_contracts": mine.len(),
        "total_contract_value": wire(&value),
        "revenue_recognized": wire(&recognised),
        "deferred_revenue": wire(&deferred),
    }))
}

/// Read an amount off a payload, refusing a missing one where the endpoint requires it.
pub fn required_amount(payload: &Value, key: &str) -> Result<Money, StoreError> {
    amount(payload, key)
}

/// The Book these tests work in, and the documents they read back.
#[cfg(test)]
mod tests_support {
    use super::*;
    use vimbai_store::fake::MemoryGraph;
    use vimbai_store::read::field;

    pub(super) fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    pub(super) fn money(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    /// A Book with a chart and one contract written into it, for the tests that post.
    pub(super) fn contract_of(parts: &[(&str, &str, &str)]) -> (MemoryGraph, String) {
        let graph = MemoryGraph::new();
        crate::chart::seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        let obligations: Vec<Value> = parts
            .iter()
            .map(|(description, price, standalone)| {
                json!({"description": description, "transaction_price": price,
                       "standalone_selling_price": standalone})
            })
            .collect();
        let written = create_contract(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "customer_name": "Acme", "contract_date": "2026-03-01",
                "obligations": obligations,
            }),
        )
        .expect("a contract");
        (graph, field(&written, "id").to_string())
    }

    pub(super) fn entry_lines(graph: &MemoryGraph, entry_id: &str) -> Vec<Value> {
        let entry = crate::ledger_book::get_entry(graph, &scope(), entry_id).expect("the entry");
        entry["lines"].as_array().cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn row(price: &str, standalone: &str) -> Obligation {
        Obligation {
            description: "obligation".into(),
            price: money(price),
            standalone: money(standalone),
            method: "point_in_time".into(),
            satisfied: false,
            recognised: Money::zero(),
        }
    }

    #[test]
    fn allocation_splits_the_price_by_relative_standalone_value() {
        let mut rows = vec![row("100", "80"), row("100", "20")];
        allocate_price(&money("200"), &mut rows);
        assert_eq!(wire(&rows[0].price), "160.000000");
        assert_eq!(wire(&rows[1].price), "40.000000");
    }

    #[test]
    fn an_obligation_without_a_standalone_price_keeps_the_price_it_was_given() {
        let mut rows = vec![row("100", "80"), row("100", "0")];
        allocate_price(&money("200"), &mut rows);
        assert_eq!(wire(&rows[0].price), "200.000000");
        assert_eq!(wire(&rows[1].price), "100.000000");
        // the parts no longer add back to the total: shipped behaviour, pinned here
        let parts = total_of(&rows);
        assert_eq!(wire(&parts), "300.000000");
    }

    #[test]
    fn recognition_stops_at_what_the_obligation_is_worth() {
        let mut only = row("100", "0");
        recognise(&mut only, &money("150"));
        assert_eq!(wire(&only.recognised), "100.000000");
        assert!(only.satisfied);
    }

    #[test]
    fn a_call_with_no_amount_recognises_the_obligation_in_full() {
        let mut only = row("40", "0");
        recognise(&mut only, &Money::zero());
        assert_eq!(wire(&only.recognised), "40.000000");
        assert!(only.satisfied);
    }

    #[test]
    fn creating_a_contract_records_its_price_and_what_is_deferred() {
        let graph = FakeGraph::returning(vec![json!({"id": "con-1"})]);
        create_contract(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "customer_name": "Acme", "contract_date": "2026-03-01",
                "obligations": [
                    {"description": "Install", "transaction_price": "1000", "standalone_selling_price": "1200"},
                    {"description": "Support", "transaction_price": "1000", "standalone_selling_price": "800"}
                ]
            }),
        )
        .expect("created");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["total_transaction_price"], json!("2000.000000"));
        assert_eq!(written["deferred_revenue"], json!("2000.000000"));
        let rows = written["obligations"].as_array().expect("rows");
        assert_eq!(rows[0]["transaction_price"], json!("1200.000000"));
        assert_eq!(rows[1]["transaction_price"], json!("800.000000"));
    }

    #[test]
    fn recognising_writes_the_contract_back_with_what_is_left_deferred() {
        let (graph, contract) =
            contract_of(&[("Install", "1200", "1200"), ("Support", "800", "800")]);
        let answer = recognise_revenue(&graph, &scope(), &contract, "0", &money("500"), &json!({}))
            .expect("recognised");
        assert_eq!(answer["recognized"], json!("500.000000"));
        assert_eq!(answer["contract_total_recognized"], json!("500.000000"));
        assert_eq!(answer["deferred"], json!("1500.000000"));
        assert_eq!(answer["is_satisfied"], json!(false));
        let written = Records::new(&graph, REVENUE_CONTRACT)
            .get(&scope(), &contract)
            .expect("the contract");
        assert_eq!(written["deferred_revenue"], json!("1500.000000"));
    }

    #[test]
    fn a_contract_past_double_size_keeps_every_digit() {
        let (graph, contract) = contract_of(&[("One", "123456789012345678901.55", "0")]);
        let answer =
            recognise_revenue(&graph, &scope(), &contract, "0", &money("0.45"), &json!({}))
                .expect("recognised");
        assert_eq!(answer["recognized"], json!("0.450000"));
        assert_eq!(answer["deferred"], json!("123456789012345678901.100000"));
    }

    #[test]
    fn an_obligation_that_does_not_exist_is_refused() {
        let (graph, contract) = contract_of(&[("Only", "10", "0")]);
        assert!(matches!(
            recognise_revenue(&graph, &scope(), &contract, "3", &money("1"), &json!({})),
            Err(StoreError::NotFound)
        ));
        // and nothing was posted for a call that recognised nothing
        assert!(
            crate::ledger_book::list_entries(&graph, &scope(), &json!({}))
                .expect("entries")
                .as_array()
                .expect("rows")
                .is_empty()
        );
    }
}

#[cfg(test)]
mod posting_tests {
    use super::tests_support::*;
    use super::*;
    use serde_json::json;
    use vimbai_store::read::field;

    #[test]
    fn recognised_revenue_moves_from_the_customer_to_sales() {
        let (graph, contract) = contract_of(&[("Install", "1200", "1200")]);
        let answer = recognise_revenue(&graph, &scope(), &contract, "0", &money("500"), &json!({}))
            .expect("recognised");

        // the customer owes it and the business earned it, found in the Book's own chart rather than hard-coded
        let lines = entry_lines(&graph, field(&answer, "journal_entry_id"));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            field(&lines[0], "account_number"),
            "1100",
            "the customer owes it"
        );
        assert_eq!(lines[0]["debit"], json!("500.000000"));
        assert_eq!(
            field(&lines[1], "account_number"),
            "4000",
            "the business earned it"
        );
        assert_eq!(lines[1]["credit"], json!("500.000000"));
        assert_eq!(answer["recognized_now"], json!("500.000000"));
    }

    #[test]
    fn only_what_was_earned_now_is_posted() {
        let (graph, contract) = contract_of(&[("Install", "1200", "1200")]);
        recognise_revenue(&graph, &scope(), &contract, "0", &money("500"), &json!({}))
            .expect("first instalment");
        let second = recognise_revenue(&graph, &scope(), &contract, "0", &money("700"), &json!({}))
            .expect("second instalment");
        // 1200 was the price, so 500 then 700 recognises it in full and posts 700 the second time, not 1200
        assert_eq!(second["recognized"], json!("1200.000000"));
        assert_eq!(second["recognized_now"], json!("700.000000"));
        let lines = entry_lines(&graph, field(&second, "journal_entry_id"));
        assert_eq!(lines[0]["debit"], json!("700.000000"));
        assert_eq!(second["is_satisfied"], json!(true));
    }

    #[test]
    fn the_same_recognition_twice_is_refused_rather_than_counted_twice() {
        let (graph, contract) = contract_of(&[("Install", "1200", "1200")]);
        recognise_revenue(&graph, &scope(), &contract, "0", &money("500"), &json!({}))
            .expect("recognised");
        // recognising a further 500 would be 1000 of a 1200 obligation: a real second instalment, and it posts
        let again = recognise_revenue(&graph, &scope(), &contract, "0", &money("500"), &json!({}))
            .expect("a second instalment");
        assert_eq!(again["recognized_now"], json!("500.000000"));
        assert_ne!(again["journal_entry_id"], Value::Null);

        // and a call that recognises nothing new posts nothing
        let third = recognise_revenue(&graph, &scope(), &contract, "0", &money("900"), &json!({}))
            .expect("nothing left to earn");
        assert_eq!(
            third["recognized_now"],
            json!("200.000000"),
            "capped at the price"
        );
    }

    #[test]
    fn a_recognition_dated_in_a_closed_month_is_refused_in_words() {
        let (graph, contract) = contract_of(&[("Install", "1200", "1200")]);
        crate::periods::close_period(
            &graph,
            &scope(),
            &json!({"period": "2026-03", "closes_on": "2026-03-31"}),
        )
        .expect("a closed month");
        let refused = recognise_revenue(
            &graph,
            &scope(),
            &contract,
            "0",
            &money("500"),
            &json!({"entry_date": "2026-03-15"}),
        )
        .expect_err("a closed month");
        let said = refused.to_string();
        assert!(said.contains("closed") || said.contains("open"), "{said}");
        // and nothing was posted into it
        assert!(
            crate::ledger_book::list_entries(&graph, &scope(), &json!({}))
                .expect("entries")
                .as_array()
                .expect("rows")
                .is_empty()
        );
    }

    #[test]
    fn a_book_with_nowhere_to_put_the_debt_says_so() {
        // a Book with no chart at all has no receivable account, and the refusal names what is missing
        let graph = vimbai_store::fake::MemoryGraph::new();
        let written = create_contract(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "customer_name": "Acme", "contract_date": "2026-03-01",
                "obligations": [{"description": "Install", "transaction_price": "100",
                                 "standalone_selling_price": "0"}],
            }),
        )
        .expect("a contract");
        let refused = recognise_revenue(
            &graph,
            &scope(),
            field(&written, "id"),
            "0",
            &money("50"),
            &json!({}),
        )
        .expect_err("no chart");
        let said = refused.to_string();
        assert!(said.contains("receivable"), "{said}");
    }

    #[test]
    fn the_day_can_be_said_and_is_the_day_the_entry_carries() {
        let (graph, contract) = contract_of(&[("Install", "1200", "1200")]);
        let answer = recognise_revenue(
            &graph,
            &scope(),
            &contract,
            "0",
            &money("500"),
            &json!({"entry_date": "2026-04-02"}),
        )
        .expect("recognised");
        let entry =
            crate::ledger_book::get_entry(&graph, &scope(), field(&answer, "journal_entry_id"))
                .expect("the entry");
        assert_eq!(entry["entry_date"], json!("2026-04-02"));
    }
}
